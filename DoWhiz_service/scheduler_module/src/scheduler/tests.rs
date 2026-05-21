use chrono::{DateTime, Utc};
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

use crate::channel::{Channel, ChannelMetadata};
use crate::mongo_store::{create_client_from_env, database_from_env};

use super::{
    actions::{
        apply_scheduler_actions, resolve_schedule_request_with_context, schedule_send_email,
    },
    core::execution_terminal_outcome,
    is_user_visible_routine_task, maybe_repair_legacy_weekday_cron_task, prepare_task_for_resume,
    snapshot::build_scheduler_snapshot,
    RunTaskTask, Schedule, ScheduledTask, Scheduler, SchedulerError, TaskExecution, TaskExecutor,
    TaskKind,
};

#[derive(Default)]
struct NoopExecutor;

impl TaskExecutor for NoopExecutor {
    fn execute(&self, _task: &TaskKind) -> Result<TaskExecution, SchedulerError> {
        Ok(TaskExecution::empty())
    }
}

struct FailingExecutor {
    message: String,
}

impl FailingExecutor {
    fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl TaskExecutor for FailingExecutor {
    fn execute(&self, _task: &TaskKind) -> Result<TaskExecution, SchedulerError> {
        Err(SchedulerError::TaskFailed(self.message.clone()))
    }
}

#[derive(Default)]
struct TerminalFailureReplyExecutor;

impl TaskExecutor for TerminalFailureReplyExecutor {
    fn execute(&self, _task: &TaskKind) -> Result<TaskExecution, SchedulerError> {
        Ok(TaskExecution {
            terminal_status: Some("failed".to_string()),
            terminal_error_message: Some(
                "Investment analysis runners failed after all configured attempts".to_string(),
            ),
            ..TaskExecution::empty()
        })
    }
}

fn base_run_task(workspace: &Path, mail_root: &Path) -> RunTaskTask {
    RunTaskTask {
        workspace_dir: workspace.to_path_buf(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        model_name: "gpt-test".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        reply_to: vec!["user@example.com".to_string()],
        reply_from: None,
        archive_root: Some(mail_root.to_path_buf()),
        thread_id: Some("thread-test".to_string()),
        thread_epoch: Some(1),
        thread_state_path: Some(workspace.join("thread_state.json")),
        channel: Channel::default(),
        slack_team_id: None,
        employee_id: None,
        requester_identifier_type: None,
        requester_identifier: None,
        account_id: None,
        channel_metadata: Default::default(),
    }
}

fn force_one_shot_due<E: TaskExecutor>(scheduler: &mut Scheduler<E>, task_id: Uuid) {
    let index = scheduler
        .tasks
        .iter()
        .position(|task| task.id == task_id)
        .expect("task exists");
    scheduler.tasks[index].enabled = true;
    scheduler.tasks[index].schedule = Schedule::OneShot {
        run_at: Utc::now() - chrono::Duration::seconds(1),
    };
    let updated = scheduler.tasks[index].clone();
    scheduler
        .store
        .update_task(&updated)
        .expect("persist forced one-shot schedule");
}

fn parse_utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid RFC3339 timestamp")
        .with_timezone(&Utc)
}

fn write_thread_request(workspace: &Path, content: &str) {
    let incoming_email = workspace.join("incoming_email");
    fs::create_dir_all(&incoming_email).expect("incoming_email dir");
    fs::write(incoming_email.join("thread_request.md"), content).expect("thread_request");
}

fn mongo_execution_tests_enabled() -> bool {
    dotenvy::dotenv().ok();
    matches!(
        (
            std::env::var("MONGODB_URI"),
            std::env::var("MONGODB_DATABASE"),
        ),
        (Ok(uri), Ok(database)) if !uri.trim().is_empty() && !database.trim().is_empty()
    )
}

fn user_scoped_tasks_db(temp: &TempDir, user_id: &str) -> PathBuf {
    let tasks_db = temp
        .path()
        .join("users")
        .join(user_id)
        .join("state")
        .join("tasks.db");
    fs::create_dir_all(tasks_db.parent().expect("tasks db parent")).expect("create state dir");
    tasks_db
}

fn load_execution_documents(user_id: &str, task_id: Uuid) -> Vec<Document> {
    let client = create_client_from_env().expect("mongo client");
    let db = database_from_env(&client);
    let collection = db.collection::<Document>("task_executions");
    collection
        .find(
            doc! {
                "owner_scope.kind": "user",
                "owner_scope.id": user_id,
                "task_id": task_id.to_string(),
            },
            FindOptions::builder()
                .sort(doc! { "started_at": 1 })
                .build(),
        )
        .expect("find executions")
        .map(|row| row.expect("execution document"))
        .collect()
}

fn load_task_documents(user_id: &str, task_id: Uuid) -> Vec<Document> {
    let client = create_client_from_env().expect("mongo client");
    let db = database_from_env(&client);
    let collection = db.collection::<Document>("tasks");
    collection
        .find(
            doc! {
                "owner_scope.kind": "user",
                "owner_scope.id": user_id,
                "task_id": task_id.to_string(),
            },
            FindOptions::builder()
                .sort(doc! { "updated_at": 1 })
                .build(),
        )
        .expect("find tasks")
        .map(|row| row.expect("task document"))
        .collect()
}

struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[test]
fn defer_one_shot_task_by_id_pushes_run_at_forward_and_persists() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");

    let task_id = scheduler
        .add_one_shot_in(Duration::from_secs(0), TaskKind::Noop)
        .expect("add task");
    force_one_shot_due(&mut scheduler, task_id);

    let changed = scheduler
        .defer_one_shot_task_by_id(task_id, chrono::Duration::seconds(15))
        .expect("defer");
    assert!(changed, "expected defer to update one-shot run_at");

    let deferred_run_at = match &scheduler
        .tasks()
        .iter()
        .find(|task| task.id == task_id)
        .expect("task exists")
        .schedule
    {
        Schedule::OneShot { run_at } => *run_at,
        _ => panic!("expected one-shot schedule"),
    };
    assert!(
        deferred_run_at >= Utc::now() + chrono::Duration::seconds(10),
        "deferred run_at should be pushed into the future"
    );

    let reloaded = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("reload");
    let persisted_run_at = match &reloaded
        .tasks()
        .iter()
        .find(|task| task.id == task_id)
        .expect("task exists after reload")
        .schedule
    {
        Schedule::OneShot { run_at } => *run_at,
        _ => panic!("expected one-shot schedule"),
    };
    assert_eq!(
        persisted_run_at, deferred_run_at,
        "deferred run_at should persist to storage"
    );
}

#[test]
fn defer_one_shot_task_by_id_is_noop_for_cron_tasks() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");

    let task_id = scheduler
        .add_cron_task("0 * * * * *", TaskKind::Noop)
        .expect("add cron task");
    let changed = scheduler
        .defer_one_shot_task_by_id(task_id, chrono::Duration::seconds(15))
        .expect("defer should not fail");
    assert!(
        !changed,
        "cron tasks should not be deferred via one-shot API"
    );
}

#[test]
fn build_scheduler_snapshot_limits_to_window() {
    let now = Utc::now();
    let in_window = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::Noop,
        schedule: Schedule::OneShot {
            run_at: now + chrono::Duration::days(1),
        },
        enabled: true,
        created_at: now,
        last_run: None,
    };
    let out_window = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::Noop,
        schedule: Schedule::OneShot {
            run_at: now + chrono::Duration::days(10),
        },
        enabled: true,
        created_at: now,
        last_run: None,
    };

    let snapshot = build_scheduler_snapshot(&[in_window, out_window], now);
    assert!(snapshot.due.is_empty());
    assert_eq!(snapshot.upcoming.len(), 1);
    assert_eq!(snapshot.omitted_after_window, 1);
    assert_eq!(snapshot.total_enabled, 2);
}

#[test]
fn build_scheduler_snapshot_surfaces_due_tasks_with_ids() {
    let now = Utc::now();
    let due_cron = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::Noop,
        schedule: Schedule::Cron {
            expression: "0 0 16 * * *".to_string(),
            next_run: now - chrono::Duration::minutes(3),
        },
        enabled: true,
        created_at: now,
        last_run: Some(now - chrono::Duration::days(1)),
    };
    let future_one_shot = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::Noop,
        schedule: Schedule::OneShot {
            run_at: now + chrono::Duration::hours(2),
        },
        enabled: true,
        created_at: now,
        last_run: None,
    };

    let snapshot = build_scheduler_snapshot(&[due_cron.clone(), future_one_shot], now);
    assert_eq!(snapshot.total_enabled, 2);
    assert_eq!(snapshot.due.len(), 1);
    assert_eq!(snapshot.due[0].id, due_cron.id.to_string());
    assert_eq!(snapshot.due[0].status, "due");
    assert_eq!(snapshot.omitted_past_due, 0);
    assert_eq!(snapshot.upcoming.len(), 1);
}

#[test]
fn apply_scheduler_actions_cancels_and_reschedules() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
    let now = Utc::now();

    let cancel_id = scheduler
        .add_one_shot_at(now + chrono::Duration::days(1), TaskKind::Noop)
        .expect("cancel task");
    let resched_id = scheduler
        .add_one_shot_at(now + chrono::Duration::days(2), TaskKind::Noop)
        .expect("resched task");

    let workspace = temp.path().join("workspaces").join("thread_1");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");
    let run_task = base_run_task(&workspace, &mail_root);

    let new_run_at = (now + chrono::Duration::days(3)).to_rfc3339();
    let actions = vec![
        run_task_module::SchedulerActionRequest::Cancel {
            task_ids: vec![cancel_id.to_string()],
        },
        run_task_module::SchedulerActionRequest::Reschedule {
            task_id: resched_id.to_string(),
            schedule: run_task_module::ScheduleRequest::OneShot { run_at: new_run_at },
        },
    ];

    apply_scheduler_actions(&mut scheduler, &run_task, &actions).expect("apply actions");

    let canceled = scheduler
        .tasks()
        .iter()
        .find(|task| task.id == cancel_id)
        .expect("cancel task found");
    assert!(!canceled.enabled);

    let rescheduled = scheduler
        .tasks()
        .iter()
        .find(|task| task.id == resched_id)
        .expect("resched task found");
    match &rescheduled.schedule {
        Schedule::OneShot { run_at } => {
            assert!(*run_at >= now + chrono::Duration::days(3));
        }
        _ => panic!("expected one_shot schedule"),
    }
    assert!(rescheduled.enabled);
}

#[test]
fn apply_scheduler_actions_creates_run_task() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
    let now = Utc::now();

    let workspace = temp.path().join("workspaces").join("thread_1");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");
    let run_task = base_run_task(&workspace, &mail_root);

    let run_at = (now + chrono::Duration::hours(2)).to_rfc3339();
    let actions = vec![run_task_module::SchedulerActionRequest::CreateRunTask {
        schedule: run_task_module::ScheduleRequest::OneShot { run_at },
        model_name: None,
        codex_disabled: None,
        reply_to: Vec::new(),
    }];

    apply_scheduler_actions(&mut scheduler, &run_task, &actions).expect("apply actions");

    assert_eq!(scheduler.tasks().len(), 1);
    match &scheduler.tasks()[0].kind {
        TaskKind::RunTask(task) => {
            assert_eq!(task.workspace_dir, workspace);
            assert_eq!(task.model_name, "gpt-test");
        }
        _ => panic!("expected run_task kind"),
    }
}

#[test]
fn resolve_schedule_request_with_context_normalizes_legacy_weekday_cron() {
    let now = parse_utc("2026-04-02T20:00:00Z");
    let schedule = run_task_module::ScheduleRequest::Cron {
        expression: "0 0 16 * * 1-5".to_string(),
    };

    let resolved = resolve_schedule_request_with_context(
        &schedule,
        now,
        Some("Please send this every weekday at 9:00 AM America/Los_Angeles."),
    )
    .expect("resolve schedule");

    match resolved {
        Schedule::Cron {
            expression,
            next_run,
        } => {
            assert_eq!(expression, "0 0 16 * * MON-FRI");
            assert_eq!(next_run, parse_utc("2026-04-03T16:00:00Z"));
        }
        _ => panic!("expected cron schedule"),
    }
}

#[test]
fn resolve_schedule_request_with_context_keeps_explicit_sunday_thursday_cron() {
    let now = parse_utc("2026-04-02T20:00:00Z");
    let schedule = run_task_module::ScheduleRequest::Cron {
        expression: "0 0 16 * * 1-5".to_string(),
    };

    let resolved = resolve_schedule_request_with_context(
        &schedule,
        now,
        Some("Please send this every Sunday through Thursday at 9:00 AM."),
    )
    .expect("resolve schedule");

    match resolved {
        Schedule::Cron {
            expression,
            next_run,
        } => {
            assert_eq!(expression, "0 0 16 * * 1-5");
            assert_eq!(next_run, parse_utc("2026-04-05T16:00:00Z"));
        }
        _ => panic!("expected cron schedule"),
    }
}

#[test]
fn maybe_repair_legacy_weekday_cron_task_uses_thread_request_context() {
    let temp = TempDir::new().expect("tempdir");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail root");
    write_thread_request(
        &workspace,
        "Track GLD for me every weekday at 9:00 AM America/Los_Angeles.",
    );

    let now = parse_utc("2026-04-02T20:00:00Z");
    let run_task = base_run_task(&workspace, &mail_root);
    let mut task = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task),
        schedule: Schedule::Cron {
            expression: "0 0 16 * * 1-5".to_string(),
            next_run: parse_utc("2026-04-05T16:00:00Z"),
        },
        enabled: true,
        created_at: now,
        last_run: None,
    };

    let repaired = maybe_repair_legacy_weekday_cron_task(&mut task, now).expect("repair task");
    assert!(repaired, "weekday request should repair the legacy cron");

    match task.schedule {
        Schedule::Cron {
            expression,
            next_run,
        } => {
            assert_eq!(expression, "0 0 16 * * MON-FRI");
            assert_eq!(next_run, parse_utc("2026-04-03T16:00:00Z"));
        }
        _ => panic!("expected cron schedule"),
    }
}

#[test]
fn schedule_send_email_supports_five_and_twenty_minute_reminders() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");

    let workspace = temp.path().join("workspaces").join("thread_1");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(workspace.join("reminder_email_attachments")).expect("attachments");
    fs::create_dir_all(&mail_root).expect("mail");
    fs::write(
        workspace.join("reminder_email_draft.html"),
        "<html><body>Reminder</body></html>",
    )
    .expect("html");

    let run_task = base_run_task(&workspace, &mail_root);

    let request_5 = run_task_module::ScheduledSendEmailTask {
        subject: "Reminder in 5 minutes".to_string(),
        html_path: "reminder_email_draft.html".to_string(),
        attachments_dir: Some("reminder_email_attachments".to_string()),
        from: None,
        to: vec!["user@example.com".to_string()],
        cc: Vec::new(),
        bcc: Vec::new(),
        delay_minutes: Some(5),
        delay_seconds: None,
        run_at: None,
    };
    let request_20 = run_task_module::ScheduledSendEmailTask {
        subject: "Reminder in 20 minutes".to_string(),
        html_path: "reminder_email_draft.html".to_string(),
        attachments_dir: Some("reminder_email_attachments".to_string()),
        from: None,
        to: vec!["user@example.com".to_string()],
        cc: Vec::new(),
        bcc: Vec::new(),
        delay_minutes: Some(20),
        delay_seconds: None,
        run_at: None,
    };

    let now_before_first = Utc::now();
    assert!(schedule_send_email(&mut scheduler, &run_task, &request_5).expect("schedule 5"));
    let now_before_second = Utc::now();
    assert!(schedule_send_email(&mut scheduler, &run_task, &request_20).expect("schedule 20"));
    let now_after_second = Utc::now();

    let mut five_min_run_at = None;
    let mut twenty_min_run_at = None;
    for task in scheduler.tasks() {
        if let TaskKind::SendReply(send_task) = &task.kind {
            if send_task.subject == "Reminder in 5 minutes" {
                if let Schedule::OneShot { run_at } = task.schedule.clone() {
                    five_min_run_at = Some(run_at);
                }
            }
            if send_task.subject == "Reminder in 20 minutes" {
                if let Schedule::OneShot { run_at } = task.schedule.clone() {
                    twenty_min_run_at = Some(run_at);
                }
            }
        }
    }

    let five_min_run_at = five_min_run_at.expect("5 minute task");
    let twenty_min_run_at = twenty_min_run_at.expect("20 minute task");

    let min_5 = now_before_first + chrono::Duration::minutes(5);
    let max_5 = now_before_second + chrono::Duration::minutes(5) + chrono::Duration::seconds(5);
    assert!(
        five_min_run_at >= min_5 && five_min_run_at <= max_5,
        "5-minute reminder run_at out of range: {} not in [{}, {}]",
        five_min_run_at,
        min_5,
        max_5
    );

    let min_20 = now_before_second + chrono::Duration::minutes(20);
    let max_20 = now_after_second + chrono::Duration::minutes(20) + chrono::Duration::seconds(5);
    assert!(
        twenty_min_run_at >= min_20 && twenty_min_run_at <= max_20,
        "20-minute reminder run_at out of range: {} not in [{}, {}]",
        twenty_min_run_at,
        min_20,
        max_20
    );

    let gap = twenty_min_run_at - five_min_run_at;
    assert!(
        gap >= chrono::Duration::minutes(14) && gap <= chrono::Duration::minutes(16),
        "expected ~15 minute gap between reminders, got {} seconds",
        gap.num_seconds()
    );
}

#[test]
fn add_one_shot_in_with_id_uses_specified_id() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");

    // Create a specific UUID
    let specific_id = Uuid::new_v4();

    // Add task with specific ID
    scheduler
        .add_one_shot_in_with_id(specific_id, Duration::from_secs(0), TaskKind::Noop)
        .expect("add task with id");

    // Verify the task has the specified ID
    assert_eq!(scheduler.tasks().len(), 1);
    assert_eq!(scheduler.tasks()[0].id, specific_id);
}

#[test]
fn same_task_id_can_be_used_in_different_schedulers() {
    let temp = TempDir::new().expect("tempdir");

    // Create two separate tasks.db files (simulating workspace and user storage)
    let workspace_db = temp.path().join("workspace_tasks.db");
    let user_db = temp.path().join("user_tasks.db");

    let mut workspace_scheduler =
        Scheduler::load(&workspace_db, NoopExecutor::default()).expect("load workspace");
    let mut user_scheduler = Scheduler::load(&user_db, NoopExecutor::default()).expect("load user");

    // Add task to workspace scheduler (generates new ID)
    let task_id = workspace_scheduler
        .add_one_shot_in(Duration::from_secs(0), TaskKind::Noop)
        .expect("add to workspace");

    // Add same task to user scheduler with the SAME ID
    user_scheduler
        .add_one_shot_in_with_id(task_id, Duration::from_secs(0), TaskKind::Noop)
        .expect("add to user with same id");

    // Verify both schedulers have a task with the same ID
    assert_eq!(workspace_scheduler.tasks().len(), 1);
    assert_eq!(user_scheduler.tasks().len(), 1);
    assert_eq!(workspace_scheduler.tasks()[0].id, task_id);
    assert_eq!(user_scheduler.tasks()[0].id, task_id);
}

#[test]
fn add_one_shot_in_with_id_persists_to_database() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let specific_id = Uuid::new_v4();

    // Add task with specific ID
    {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in_with_id(specific_id, Duration::from_secs(0), TaskKind::Noop)
            .expect("add task");
    }

    // Reload scheduler and verify task is still there with correct ID
    {
        let scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("reload");
        assert_eq!(scheduler.tasks().len(), 1);
        assert_eq!(scheduler.tasks()[0].id, specific_id);
    }
}

#[test]
fn add_one_shot_in_if_absent_with_id_skips_existing_task() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let specific_id = Uuid::new_v4();

    {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        let inserted = scheduler
            .add_one_shot_in_if_absent_with_id(specific_id, Duration::from_secs(0), TaskKind::Noop)
            .expect("insert first task");
        assert!(inserted, "first insertion should create the task");

        let inserted_again = scheduler
            .add_one_shot_in_if_absent_with_id(specific_id, Duration::from_secs(0), TaskKind::Noop)
            .expect("second insert should not fail");
        assert!(
            !inserted_again,
            "duplicate insertion should be skipped for the same task id"
        );
        assert_eq!(scheduler.tasks().len(), 1);
    }

    let scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("reload");
    assert_eq!(scheduler.tasks().len(), 1);
    assert_eq!(scheduler.tasks()[0].id, specific_id);
}

#[test]
fn execution_status_can_be_recorded_for_task() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let specific_id = Uuid::new_v4();

    // Add task and record execution
    {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in_with_id(specific_id, Duration::from_secs(0), TaskKind::Noop)
            .expect("add task");

        // Record execution start and finish (this is what sync_task_status_to_user_storage does)
        let now = Utc::now();
        let execution_id = scheduler
            .store
            .record_execution_start(specific_id, now)
            .expect("record start");
        scheduler
            .store
            .record_execution_finish(specific_id, execution_id, now, "success", None)
            .expect("record finish");
    }

    // Verify execution status is persisted by loading tasks with status
    {
        use super::store::SchedulerStore;
        let store = SchedulerStore::new(tasks_db).expect("open store");
        let tasks = store.list_tasks_with_status().expect("list tasks");

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, specific_id.to_string());
        assert_eq!(tasks[0].execution_status, Some("success".to_string()));
    }
}

#[test]
fn terminal_failure_reply_outcome_uses_failed_status_and_error_note() {
    let execution = TaskExecution {
        terminal_status: Some("failed".to_string()),
        terminal_error_message: Some(
            "Investment analysis runners failed after all configured attempts".to_string(),
        ),
        terminal_note: Some("generic recovery note".to_string()),
        ..TaskExecution::empty()
    };

    let outcome = execution_terminal_outcome(&execution);
    assert_eq!(outcome.status, "failed");
    assert_eq!(
        outcome.note.as_deref(),
        Some("Investment analysis runners failed after all configured attempts")
    );
}

#[test]
fn terminal_failure_reply_is_sent_but_run_task_status_is_failed() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("skipping Mongo-backed scheduler integration test: MONGODB_URI/MONGODB_DATABASE not set");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");
    fs::write(
        workspace.join("reply_email_draft.html"),
        "<html><body><p>Analysis could not be completed.</p></body></html>",
    )
    .expect("reply draft");

    let run_task = base_run_task(&workspace, &mail_root);
    let task_id = {
        let mut scheduler =
            Scheduler::load(&tasks_db, TerminalFailureReplyExecutor).expect("load scheduler");
        let task_id = scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))
            .expect("add run task");
        scheduler.tick().expect("tick");
        task_id
    };

    let store = SchedulerStore::new(tasks_db).expect("open store");
    let tasks = store.list_tasks_with_status().expect("list tasks");
    let run_task_summary = tasks
        .iter()
        .find(|task| task.id == task_id.to_string())
        .expect("run task summary");
    assert_eq!(run_task_summary.execution_status.as_deref(), Some("failed"));
    assert!(
        run_task_summary
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("Investment analysis runners failed"),
        "terminal failure message should be visible on the failed run task"
    );
    assert!(
        tasks
            .iter()
            .any(|task| task.kind == "send_email" && task.channel == "email"),
        "terminal failure reply should still schedule an outbound email"
    );
}

#[test]
fn reconcile_stale_running_execution_supersedes_older_duplicate() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "stale-duplicate-user");
    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::Noop)
            .expect("add task")
    };

    let store = SchedulerStore::new(tasks_db).expect("open store");
    let started_at_1 = parse_utc("2026-04-01T00:00:00Z");
    let started_at_2 = parse_utc("2026-04-01T00:05:00Z");
    store
        .record_execution_start(task_id, started_at_1)
        .expect("record start 1");
    let running_handle = store
        .record_execution_start(task_id, started_at_2)
        .expect("record start 2");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-01T00:10:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 1);
    assert_eq!(summary.failed_count, 0);
    assert!(store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let executions = load_execution_documents("stale-duplicate-user", task_id);
    assert_eq!(executions.len(), 2);
    assert_eq!(
        executions[0].get_str("status").expect("status"),
        "superseded"
    );
    assert_eq!(executions[1].get_str("status").expect("status"), "running");
    assert_eq!(
        executions[1].get_i64("execution_id").expect("execution id"),
        running_handle.execution_id
    );
}

#[test]
fn reconcile_stale_running_execution_fails_orphaned_latest_row_after_timeout() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "stale-timeout-user");
    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::Noop)
            .expect("add task")
    };

    let store = SchedulerStore::new(tasks_db).expect("open store");
    store
        .record_execution_start(task_id, parse_utc("2026-04-01T00:00:00Z"))
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-02T02:00:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 0);
    assert_eq!(summary.failed_count, 1);
    assert!(!store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let executions = load_execution_documents("stale-timeout-user", task_id);
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].get_str("status").expect("status"), "failed");
}

#[test]
fn reconcile_stale_running_execution_keeps_active_fallback_open() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let _lock = env_lock();
    let _aci_rg = EnvGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "stale-active-fallback-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add task")
    };

    let primary_aci_dir = workspace.join(".run_task_trace_codex_primary/aci");
    fs::create_dir_all(&primary_aci_dir).expect("primary trace dir");
    fs::write(primary_aci_dir.join("container_show.json"), "{}").expect("container show");

    let fallback_trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&fallback_trace_dir).expect("fallback trace dir");
    fs::write(
        fallback_trace_dir.join("metadata.json"),
        r#"{
  "backend": "claude_local",
  "current_stage": "executing_claude_local",
  "finished_at_unix_ms": null,
  "success": null
}"#,
    )
    .expect("fallback metadata");

    let store = SchedulerStore::new(tasks_db).expect("open store");
    store
        .record_execution_start(task_id, parse_utc("2026-04-01T00:00:00Z"))
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-01T00:10:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 0);
    assert_eq!(summary.failed_count, 0);
    assert!(store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let executions = load_execution_documents("stale-active-fallback-user", task_id);
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].get_str("status").expect("status"), "running");
}

#[test]
fn reconcile_stale_running_execution_fails_hung_fallback_after_default_timeout() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let _lock = env_lock();
    let _aci_rg = EnvGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "stale-hung-fallback-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add task")
    };

    let primary_aci_dir = workspace.join(".run_task_trace_codex_primary/aci");
    fs::create_dir_all(&primary_aci_dir).expect("primary trace dir");
    fs::write(primary_aci_dir.join("container_show.json"), "{}").expect("container show");

    let fallback_trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&fallback_trace_dir).expect("fallback trace dir");
    fs::write(
        fallback_trace_dir.join("metadata.json"),
        r#"{
  "backend": "claude_local",
  "current_stage": "executing_claude_local",
  "finished_at_unix_ms": null,
  "success": null
}"#,
    )
    .expect("fallback metadata");

    let store = SchedulerStore::new(tasks_db).expect("open store");
    store
        .record_execution_start(task_id, parse_utc("2026-04-01T00:00:00Z"))
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-01T00:20:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 0);
    assert_eq!(summary.failed_count, 1);
    assert!(!store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let executions = load_execution_documents("stale-hung-fallback-user", task_id);
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].get_str("status").expect("status"), "failed");
    assert_eq!(
        executions[0].get_str("error_message").expect("error"),
        "reconciled stale running execution after primary ACI run failed and fallback never reached a terminal state"
    );
}

#[test]
fn reconcile_stale_running_execution_keeps_active_aci_result_handling_open() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let _lock = env_lock();
    let _aci_rg = EnvGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "stale-active-result-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add task")
    };

    let started_at = parse_utc("2026-04-01T00:00:00Z");
    let activity_at = parse_utc("2026-04-01T00:15:00Z");
    let trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&trace_dir).expect("trace dir");
    fs::write(
        trace_dir.join("metadata.json"),
        format!(
            r#"{{
  "backend": "codex_azure_aci",
  "current_stage": "downloading_results",
  "started_at_unix_ms": {},
  "stage_updated_at_unix_ms": {},
  "finished_at_unix_ms": null,
  "success": null
}}"#,
            started_at.timestamp_millis(),
            activity_at.timestamp_millis()
        ),
    )
    .expect("trace metadata");

    let store = SchedulerStore::new(tasks_db).expect("open store");
    store
        .record_execution_start(task_id, started_at)
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-01T00:20:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 0);
    assert_eq!(summary.failed_count, 0);
    assert!(store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let executions = load_execution_documents("stale-active-result-user", task_id);
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].get_str("status").expect("status"), "running");
}

#[test]
fn reconcile_stale_running_execution_fails_abandoned_fallback_after_watchdog() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let _lock = env_lock();
    let _aci_rg = EnvGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "stale-abandoned-fallback-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add task")
    };

    let primary_aci_dir = workspace.join(".run_task_trace_codex_primary/aci");
    fs::create_dir_all(&primary_aci_dir).expect("primary trace dir");
    fs::write(primary_aci_dir.join("container_show.json"), "{}").expect("container show");

    let fallback_trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&fallback_trace_dir).expect("fallback trace dir");
    fs::write(
        fallback_trace_dir.join("metadata.json"),
        r#"{
  "backend": "claude_local",
  "current_stage": "executing_claude_local",
  "finished_at_unix_ms": null,
  "success": null
}"#,
    )
    .expect("fallback metadata");

    let store = SchedulerStore::new(tasks_db).expect("open store");
    store
        .record_execution_start(task_id, parse_utc("2026-04-01T00:00:00Z"))
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-02T02:00:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 0);
    assert_eq!(summary.failed_count, 1);
    assert!(!store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let executions = load_execution_documents("stale-abandoned-fallback-user", task_id);
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].get_str("status").expect("status"), "failed");
    assert_eq!(
        executions[0].get_str("error_message").expect("error"),
        "reconciled stale running execution after primary ACI run failed and fallback never reached a terminal state"
    );
}

#[test]
fn reconcile_stale_running_execution_supersedes_overlap_after_later_completion() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "stale-overlap-user");
    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::Noop)
            .expect("add task")
    };

    let store = SchedulerStore::new(tasks_db).expect("open store");
    let successful_handle = store
        .record_execution_start(task_id, parse_utc("2026-04-01T00:00:00Z"))
        .expect("record start success");
    store
        .record_execution_start(task_id, parse_utc("2026-04-01T00:05:00Z"))
        .expect("record stale start");
    store
        .record_execution_finish(
            task_id,
            successful_handle,
            parse_utc("2026-04-01T00:10:00Z"),
            "success",
            None,
        )
        .expect("record finish success");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-01T00:15:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 1);
    assert_eq!(summary.failed_count, 0);
    assert!(!store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let executions = load_execution_documents("stale-overlap-user", task_id);
    assert_eq!(executions.len(), 2);
    assert_eq!(executions[0].get_str("status").expect("status"), "success");
    assert_eq!(
        executions[1].get_str("status").expect("status"),
        "superseded"
    );
}

#[test]
fn reconcile_stale_running_execution_supersedes_duplicate_workspace_task() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "duplicate-workspace-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let (task_id_a, task_id_b) = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        let first = scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add first task");
        let second = scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add second task");
        (first, second)
    };

    let store = SchedulerStore::new(tasks_db).expect("open store");
    store
        .record_execution_start(task_id_a, parse_utc("2026-04-01T00:00:00Z"))
        .expect("record first start");
    store
        .record_execution_start(task_id_b, parse_utc("2026-04-01T00:00:30Z"))
        .expect("record second start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id_b.to_string(),
            parse_utc("2026-04-01T00:20:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");

    assert_eq!(summary.superseded_count, 1);
    assert_eq!(summary.failed_count, 0);
    assert!(store
        .has_running_execution(&task_id_a.to_string())
        .expect("first still running"));
    assert!(!store
        .has_running_execution(&task_id_b.to_string())
        .expect("second closed"));

    let executions_b = load_execution_documents("duplicate-workspace-user", task_id_b);
    assert_eq!(executions_b.len(), 1);
    assert_eq!(
        executions_b[0].get_str("status").expect("status"),
        "superseded"
    );
    assert!(executions_b[0]
        .get_str("error_message")
        .expect("error")
        .contains("another task for the same workspace was already running"));
}

#[test]
fn record_execution_finish_is_idempotent_after_reconciliation_closes_row() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "late-finish-user");
    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::Noop)
            .expect("add task")
    };

    let store = SchedulerStore::new(tasks_db).expect("open store");
    let handle = store
        .record_execution_start(task_id, parse_utc("2026-04-01T00:00:00Z"))
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-02T02:00:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");
    assert_eq!(summary.failed_count, 1);

    store
        .record_execution_finish(
            task_id,
            handle,
            parse_utc("2026-04-02T02:05:00Z"),
            "failed",
            Some("late finish after reconciliation"),
        )
        .expect("late finish should be tolerated");

    let executions = load_execution_documents("late-finish-user", task_id);
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].get_str("status").expect("status"), "failed");
}

#[test]
fn record_execution_finish_replaces_stale_reconciliation_failure_with_success() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let _lock = env_lock();
    let _aci_rg = EnvGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "late-success-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add task")
    };

    let started_at = parse_utc("2026-04-01T00:00:00Z");
    let stale_finished_at = parse_utc("2026-04-01T00:19:00Z");
    let success_finished_at = parse_utc("2026-04-01T00:21:00Z");
    let trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&trace_dir).expect("trace dir");
    fs::write(
        trace_dir.join("metadata.json"),
        format!(
            r#"{{
  "backend": "codex_azure_aci",
  "current_stage": "completed",
  "started_at_unix_ms": {},
  "finished_at_unix_ms": {},
  "success": true
}}"#,
            started_at.timestamp_millis(),
            stale_finished_at.timestamp_millis()
        ),
    )
    .expect("trace metadata");

    let store = SchedulerStore::new(tasks_db).expect("open store");
    let handle = store
        .record_execution_start(task_id, started_at)
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-01T00:20:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");
    assert_eq!(summary.failed_count, 1);
    assert!(!store
        .has_running_execution(&task_id.to_string())
        .expect("running check"));

    let tasks_before = load_task_documents("late-success-user", task_id);
    assert_eq!(tasks_before.len(), 1);
    assert_eq!(
        tasks_before[0]
            .get_str("auto_disabled_reason")
            .expect("auto disabled reason"),
        "auto-disabled: execution lost terminal reconciliation after an ACI-backed runner failure"
    );

    let executions_before = load_execution_documents("late-success-user", task_id);
    assert_eq!(executions_before.len(), 1);
    assert_eq!(
        executions_before[0]
            .get_str("status")
            .expect("status before"),
        "failed"
    );
    assert_eq!(
        executions_before[0]
            .get_str("error_message")
            .expect("error before"),
        "reconciled stale running execution after an ACI-backed runner executed but no live registry record remained"
    );

    store
        .record_execution_finish(task_id, handle, success_finished_at, "success", None)
        .expect("late success should overwrite stale failure");

    let executions_after = load_execution_documents("late-success-user", task_id);
    assert_eq!(executions_after.len(), 1);
    assert_eq!(
        executions_after[0].get_str("status").expect("status after"),
        "success"
    );
    assert_eq!(
        executions_after[0]
            .get("error_message")
            .expect("error field after"),
        &Bson::Null
    );

    let tasks_after = load_task_documents("late-success-user", task_id);
    assert_eq!(tasks_after.len(), 1);
    assert!(
        !tasks_after[0].contains_key("auto_disabled_reason"),
        "late success should clear auto-disabled state"
    );
}

#[test]
fn record_execution_finish_replaces_stale_reconciliation_failure_with_real_failure() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let _lock = env_lock();
    let _aci_rg = EnvGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "late-real-failure-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add task")
    };

    let started_at = parse_utc("2026-04-01T00:00:00Z");
    let stale_finished_at = parse_utc("2026-04-01T00:19:00Z");
    let real_finished_at = parse_utc("2026-04-01T00:21:00Z");
    let trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&trace_dir).expect("trace dir");
    fs::write(
        trace_dir.join("metadata.json"),
        format!(
            r#"{{
  "backend": "codex_azure_aci",
  "current_stage": "failed",
  "started_at_unix_ms": {},
  "finished_at_unix_ms": {},
  "success": false
}}"#,
            started_at.timestamp_millis(),
            stale_finished_at.timestamp_millis()
        ),
    )
    .expect("trace metadata");
    fs::write(workspace.join(".aci_recovery_context.json"), "{}").expect("aci evidence");

    let store = SchedulerStore::new(tasks_db).expect("open store");
    let handle = store
        .record_execution_start(task_id, started_at)
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-01T00:20:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");
    assert_eq!(summary.failed_count, 1);

    store
        .record_execution_finish(
            task_id,
            handle,
            real_finished_at,
            "failed",
            Some("task execution failed: Primary runner failed:"),
        )
        .expect("late real failure should overwrite stale failure");

    let executions_after = load_execution_documents("late-real-failure-user", task_id);
    assert_eq!(executions_after.len(), 1);
    assert_eq!(
        executions_after[0].get_str("status").expect("status after"),
        "failed"
    );
    assert_eq!(
        executions_after[0]
            .get_str("error_message")
            .expect("error after"),
        "task execution failed: Primary runner failed:"
    );

    let tasks_after = load_task_documents("late-real-failure-user", task_id);
    assert_eq!(tasks_after.len(), 1);
    assert!(
        !tasks_after[0].contains_key("auto_disabled_reason"),
        "late real failure should clear stale auto-disabled state"
    );
}

#[test]
fn mark_execution_finished_by_workspace_replaces_stale_reconciliation_failure_with_success() {
    use super::mark_execution_finished_by_workspace;
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping execution reconciliation test; MongoDB config not set.");
        return;
    }

    let _lock = env_lock();
    let _aci_rg = EnvGuard::set("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");

    let temp = TempDir::new().expect("tempdir");
    let tasks_db = user_scoped_tasks_db(&temp, "recovery-success-user");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let task_id = {
        let mut scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(base_run_task(&workspace, &mail_root)),
            )
            .expect("add task")
    };

    let started_at = parse_utc("2026-04-03T00:00:00Z");
    let stale_finished_at = parse_utc("2026-04-03T00:19:00Z");
    let trace_dir = workspace.join(".run_task_trace");
    fs::create_dir_all(&trace_dir).expect("trace dir");
    fs::write(
        trace_dir.join("metadata.json"),
        format!(
            r#"{{
  "backend": "codex_azure_aci",
  "current_stage": "completed",
  "started_at_unix_ms": {},
  "finished_at_unix_ms": {},
  "success": true
}}"#,
            started_at.timestamp_millis(),
            stale_finished_at.timestamp_millis()
        ),
    )
    .expect("trace metadata");
    fs::write(workspace.join(".aci_recovery_context.json"), "{}").expect("aci evidence");

    let store = SchedulerStore::new(tasks_db).expect("open store");
    store
        .record_execution_start(task_id, started_at)
        .expect("record start");

    let summary = store
        .reconcile_stale_running_executions_for_task(
            &task_id.to_string(),
            parse_utc("2026-04-03T00:20:00Z"),
            chrono::Duration::hours(24),
        )
        .expect("reconcile");
    assert_eq!(summary.failed_count, 1);

    assert!(
        mark_execution_finished_by_workspace(&workspace, "success", None)
            .expect("workspace recovery should overwrite stale failure"),
        "workspace recovery should report that it updated a row"
    );

    let executions_after = load_execution_documents("recovery-success-user", task_id);
    assert_eq!(executions_after.len(), 1);
    assert_eq!(
        executions_after[0].get_str("status").expect("status after"),
        "success"
    );
    assert_eq!(
        executions_after[0]
            .get("error_message")
            .expect("error field after"),
        &Bson::Null
    );

    let tasks_after = load_task_documents("recovery-success-user", task_id);
    assert_eq!(tasks_after.len(), 1);
    assert!(
        !tasks_after[0].contains_key("auto_disabled_reason"),
        "workspace recovery success should clear stale auto-disabled state"
    );
}

#[test]
fn scheduler_load_ignores_zero_byte_placeholder_path() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    fs::write(&tasks_db, "").expect("create zero-byte db");

    let scheduler = Scheduler::load(&tasks_db, NoopExecutor::default()).expect("load");
    assert!(scheduler.tasks().is_empty());

    let size = fs::metadata(&tasks_db).expect("metadata").len();
    assert_eq!(
        size, 0,
        "mongo backend should not mutate placeholder state file"
    );

    let mut quarantined = false;
    for entry in fs::read_dir(temp.path()).expect("read dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("tasks.db.corrupt.") {
            quarantined = true;
            break;
        }
    }
    assert!(
        !quarantined,
        "mongo backend should not emit legacy corruption quarantine files"
    );
}

/// Helper to create a Discord-style RunTaskTask
fn discord_run_task(workspace: &Path) -> RunTaskTask {
    RunTaskTask {
        workspace_dir: workspace.to_path_buf(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        model_name: "gpt-test".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        // Discord uses reply_to[0] = user_id, reply_to[1] = channel_id
        reply_to: vec!["discord_user_123".to_string(), "channel_456".to_string()],
        reply_from: None,
        archive_root: None,
        thread_id: Some("discord:guild123:channel456:thread789".to_string()),
        thread_epoch: Some(1),
        thread_state_path: Some(workspace.join("thread_state.json")),
        channel: Channel::Discord,
        slack_team_id: None,
        employee_id: Some("test_employee".to_string()),
        requester_identifier_type: None,
        requester_identifier: None,
        account_id: None,
        channel_metadata: Default::default(),
    }
}

/// Helper to create a Slack-style RunTaskTask
fn slack_run_task(workspace: &Path) -> RunTaskTask {
    RunTaskTask {
        workspace_dir: workspace.to_path_buf(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        model_name: "gpt-test".to_string(),
        runner: "codex".to_string(),
        codex_disabled: false,
        // Slack uses reply_to[0] = channel_id
        reply_to: vec!["C12345678".to_string()],
        reply_from: None,
        archive_root: None,
        thread_id: Some("slack:C12345678:1234567890.123456".to_string()),
        thread_epoch: Some(1),
        thread_state_path: Some(workspace.join("thread_state.json")),
        channel: Channel::Slack,
        slack_team_id: Some("T12345678".to_string()),
        employee_id: Some("test_employee".to_string()),
        requester_identifier_type: None,
        requester_identifier: None,
        account_id: None,
        channel_metadata: Default::default(),
    }
}

#[test]
fn run_task_normalized_channel_metadata_recovers_slack_workspace_context() {
    let temp = TempDir::new().expect("tempdir");
    let task = slack_run_task(temp.path());

    let metadata = task.normalized_channel_metadata();
    assert_eq!(metadata.slack_team_id.as_deref(), Some("T12345678"));
    assert_eq!(metadata.slack_channel_id.as_deref(), Some("C12345678"));
}

#[test]
fn run_task_normalized_channel_metadata_recovers_discord_guild_context() {
    let temp = TempDir::new().expect("tempdir");
    let mut task = discord_run_task(temp.path());
    task.reply_to = vec!["discord_user_123".to_string(), "456".to_string()];
    task.thread_id = Some("discord:123:456:thread789".to_string());
    task.channel_metadata = ChannelMetadata::default();

    let metadata = task.normalized_channel_metadata();
    assert_eq!(metadata.discord_guild_id, Some(123));
    assert_eq!(metadata.discord_channel_id, Some(456));
}

#[test]
fn full_discord_flow_task_sync_and_status_update() {
    let temp = TempDir::new().expect("tempdir");

    // Simulate Discord workspace and user storage paths
    let workspace_dir = temp
        .path()
        .join("workspaces")
        .join("discord")
        .join("guild123");
    let workspace_db = workspace_dir.join("state").join("tasks.db");
    let user_db = temp
        .path()
        .join("users")
        .join("account_abc")
        .join("state")
        .join("tasks.db");

    fs::create_dir_all(workspace_db.parent().unwrap()).expect("create workspace dir");
    fs::create_dir_all(user_db.parent().unwrap()).expect("create user dir");
    fs::create_dir_all(&workspace_dir).expect("create workspace");

    let run_task = discord_run_task(&workspace_dir);

    // Step 1: Create task in workspace scheduler (simulates discord.rs)
    let task_id = {
        let mut workspace_scheduler =
            Scheduler::load(&workspace_db, NoopExecutor::default()).expect("load workspace");
        workspace_scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task.clone()))
            .expect("add to workspace")
    };

    // Step 2: Create same task in user scheduler with same ID (simulates discord.rs account sync)
    {
        let mut user_scheduler =
            Scheduler::load(&user_db, NoopExecutor::default()).expect("load user");
        user_scheduler
            .add_one_shot_in_with_id(
                task_id,
                Duration::from_secs(0),
                TaskKind::RunTask(run_task.clone()),
            )
            .expect("add to user");
    }

    // Verify both have the task with same ID
    {
        use super::store::SchedulerStore;
        let workspace_store = SchedulerStore::new(workspace_db.clone()).expect("open workspace");
        let user_store = SchedulerStore::new(user_db.clone()).expect("open user");

        let workspace_tasks = workspace_store
            .list_tasks_with_status()
            .expect("list workspace");
        let user_tasks = user_store.list_tasks_with_status().expect("list user");

        assert_eq!(workspace_tasks.len(), 1);
        assert_eq!(user_tasks.len(), 1);
        assert_eq!(workspace_tasks[0].id, task_id.to_string());
        assert_eq!(user_tasks[0].id, task_id.to_string());
        assert_eq!(workspace_tasks[0].channel, "discord");
        assert_eq!(user_tasks[0].channel, "discord");
        // Both should have no execution status yet
        assert!(workspace_tasks[0].execution_status.is_none());
        assert!(user_tasks[0].execution_status.is_none());
    }

    // Step 3: Simulate task execution in workspace (core.rs execute_task_at_index)
    let executed_at = Utc::now();
    {
        use super::store::SchedulerStore;
        let workspace_store = SchedulerStore::new(workspace_db.clone()).expect("open workspace");
        let execution_id = workspace_store
            .record_execution_start(task_id, executed_at)
            .expect("record start");
        workspace_store
            .record_execution_finish(task_id, execution_id, executed_at, "success", None)
            .expect("record finish");
    }

    // Step 4: Sync status to user storage (simulates sync_task_status_to_user_storage)
    {
        use super::store::SchedulerStore;
        let user_store = SchedulerStore::new(user_db.clone()).expect("open user");
        let execution_id = user_store
            .record_execution_start(task_id, executed_at)
            .expect("record start");
        user_store
            .record_execution_finish(task_id, execution_id, executed_at, "success", None)
            .expect("record finish");
    }

    // Verify both now have success status
    {
        use super::store::SchedulerStore;
        let workspace_store = SchedulerStore::new(workspace_db).expect("open workspace");
        let user_store = SchedulerStore::new(user_db).expect("open user");

        let workspace_tasks = workspace_store
            .list_tasks_with_status()
            .expect("list workspace");
        let user_tasks = user_store.list_tasks_with_status().expect("list user");

        assert_eq!(
            workspace_tasks[0].execution_status,
            Some("success".to_string())
        );
        assert_eq!(user_tasks[0].execution_status, Some("success".to_string()));
    }
}

#[test]
fn full_slack_flow_task_sync_and_status_update() {
    let temp = TempDir::new().expect("tempdir");

    // Simulate Slack workspace (uses user paths) and account storage
    let user_workspace_dir = temp
        .path()
        .join("users")
        .join("slack_user")
        .join("workspaces")
        .join("thread1");
    let workspace_db = temp
        .path()
        .join("users")
        .join("slack_user")
        .join("state")
        .join("tasks.db");
    let account_db = temp
        .path()
        .join("users")
        .join("account_xyz")
        .join("state")
        .join("tasks.db");

    fs::create_dir_all(workspace_db.parent().unwrap()).expect("create workspace dir");
    fs::create_dir_all(account_db.parent().unwrap()).expect("create account dir");
    fs::create_dir_all(&user_workspace_dir).expect("create workspace");

    let run_task = slack_run_task(&user_workspace_dir);

    // Step 1: Create task in user's scheduler (Slack uses user paths)
    let task_id = {
        let mut scheduler = Scheduler::load(&workspace_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task.clone()))
            .expect("add task")
    };

    // Step 2: Create same task in account-level storage with same ID
    {
        let mut account_scheduler =
            Scheduler::load(&account_db, NoopExecutor::default()).expect("load account");
        account_scheduler
            .add_one_shot_in_with_id(
                task_id,
                Duration::from_secs(0),
                TaskKind::RunTask(run_task.clone()),
            )
            .expect("add to account");
    }

    // Verify both have Slack channel type
    {
        use super::store::SchedulerStore;
        let workspace_store = SchedulerStore::new(workspace_db.clone()).expect("open workspace");
        let account_store = SchedulerStore::new(account_db.clone()).expect("open account");

        let workspace_tasks = workspace_store
            .list_tasks_with_status()
            .expect("list workspace");
        let account_tasks = account_store
            .list_tasks_with_status()
            .expect("list account");

        assert_eq!(workspace_tasks[0].channel, "slack");
        assert_eq!(account_tasks[0].channel, "slack");
        assert_eq!(workspace_tasks[0].id, account_tasks[0].id);
    }

    // Step 3: Simulate failed execution
    let executed_at = Utc::now();
    let error_message = "Task failed: API timeout";
    {
        use super::store::SchedulerStore;
        let workspace_store = SchedulerStore::new(workspace_db.clone()).expect("open workspace");
        let execution_id = workspace_store
            .record_execution_start(task_id, executed_at)
            .expect("record start");
        workspace_store
            .record_execution_finish(
                task_id,
                execution_id,
                executed_at,
                "failed",
                Some(error_message),
            )
            .expect("record finish");
    }

    // Step 4: Sync failure status to account storage
    {
        use super::store::SchedulerStore;
        let account_store = SchedulerStore::new(account_db.clone()).expect("open account");
        let execution_id = account_store
            .record_execution_start(task_id, executed_at)
            .expect("record start");
        account_store
            .record_execution_finish(
                task_id,
                execution_id,
                executed_at,
                "failed",
                Some(error_message),
            )
            .expect("record finish");
    }

    // Verify both have failure status with error message
    {
        use super::store::SchedulerStore;
        let workspace_store = SchedulerStore::new(workspace_db).expect("open workspace");
        let account_store = SchedulerStore::new(account_db).expect("open account");

        let workspace_tasks = workspace_store
            .list_tasks_with_status()
            .expect("list workspace");
        let account_tasks = account_store
            .list_tasks_with_status()
            .expect("list account");

        assert_eq!(
            workspace_tasks[0].execution_status,
            Some("failed".to_string())
        );
        assert_eq!(
            account_tasks[0].execution_status,
            Some("failed".to_string())
        );
        assert_eq!(
            workspace_tasks[0].error_message,
            Some(error_message.to_string())
        );
        assert_eq!(
            account_tasks[0].error_message,
            Some(error_message.to_string())
        );
    }
}

#[test]
fn mirrored_terminal_execution_preserves_source_execution_identity() {
    use super::store::SchedulerStore;

    if !mongo_execution_tests_enabled() {
        eprintln!("Skipping mirror execution test; MongoDB config not set.");
        return;
    }

    let temp = TempDir::new().expect("tempdir");
    let workspace_db = user_scoped_tasks_db(&temp, "mirror-live-user");
    let account_db = user_scoped_tasks_db(&temp, "mirror-account-user");
    let workspace_dir = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace_dir).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");

    let run_task = base_run_task(&workspace_dir, &mail_root);
    let task_id = {
        let mut scheduler =
            Scheduler::load(&workspace_db, NoopExecutor::default()).expect("load workspace");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task.clone()))
            .expect("add workspace task")
    };

    {
        let mut scheduler =
            Scheduler::load(&account_db, NoopExecutor::default()).expect("load account");
        scheduler
            .add_one_shot_in_with_id(task_id, Duration::from_secs(0), TaskKind::RunTask(run_task))
            .expect("add account task");
    }

    let started_at = parse_utc("2026-04-01T00:00:00Z");
    let finished_at = parse_utc("2026-04-01T00:10:00Z");
    let workspace_store = SchedulerStore::new(workspace_db).expect("open workspace");
    let execution = workspace_store
        .record_execution_start(task_id, started_at)
        .expect("record source start");
    workspace_store
        .record_execution_finish(task_id, execution, finished_at, "success", None)
        .expect("record source finish");

    let account_store = SchedulerStore::new(account_db).expect("open account");
    account_store
        .upsert_terminal_execution(task_id, execution, finished_at, "success", None)
        .expect("mirror execution");
    account_store
        .upsert_terminal_execution(task_id, execution, finished_at, "success", None)
        .expect("mirror execution idempotent");

    let executions = load_execution_documents("mirror-account-user", task_id);
    assert_eq!(executions.len(), 1);
    assert_eq!(
        executions[0].get_i64("execution_id").expect("execution id"),
        execution.execution_id
    );
    assert_eq!(
        executions[0]
            .get_datetime("started_at")
            .expect("started_at")
            .to_chrono(),
        started_at
    );
    assert_eq!(
        executions[0]
            .get_datetime("finished_at")
            .expect("finished_at")
            .to_chrono(),
        finished_at
    );
    assert_eq!(executions[0].get_str("status").expect("status"), "success");
}

#[test]
fn multiple_tasks_sync_independently() {
    let temp = TempDir::new().expect("tempdir");

    let workspace_db = temp.path().join("workspace_tasks.db");
    let user_db = temp.path().join("user_tasks.db");
    let workspace_dir = temp.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create workspace");

    // Create two Discord tasks
    let run_task_1 = discord_run_task(&workspace_dir);
    let mut run_task_2 = discord_run_task(&workspace_dir);
    run_task_2.thread_id = Some("discord:guild123:channel456:thread_different".to_string());

    // Add both tasks to workspace
    let task_id_1 = {
        let mut scheduler = Scheduler::load(&workspace_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(run_task_1.clone()),
            )
            .expect("add 1")
    };
    let task_id_2 = {
        let mut scheduler = Scheduler::load(&workspace_db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(
                Duration::from_secs(0),
                TaskKind::RunTask(run_task_2.clone()),
            )
            .expect("add 2")
    };

    // Sync both to user storage
    {
        let mut user_scheduler =
            Scheduler::load(&user_db, NoopExecutor::default()).expect("load user");
        user_scheduler
            .add_one_shot_in_with_id(
                task_id_1,
                Duration::from_secs(0),
                TaskKind::RunTask(run_task_1),
            )
            .expect("sync 1");
        user_scheduler
            .add_one_shot_in_with_id(
                task_id_2,
                Duration::from_secs(0),
                TaskKind::RunTask(run_task_2),
            )
            .expect("sync 2");
    }

    // Mark task 1 as success, task 2 as failed
    let executed_at = Utc::now();
    {
        use super::store::SchedulerStore;
        let user_store = SchedulerStore::new(user_db.clone()).expect("open user");

        // Task 1: success
        let exec_id_1 = user_store
            .record_execution_start(task_id_1, executed_at)
            .expect("start 1");
        user_store
            .record_execution_finish(task_id_1, exec_id_1, executed_at, "success", None)
            .expect("finish 1");

        // Task 2: failed
        let exec_id_2 = user_store
            .record_execution_start(task_id_2, executed_at)
            .expect("start 2");
        user_store
            .record_execution_finish(task_id_2, exec_id_2, executed_at, "failed", Some("timeout"))
            .expect("finish 2");
    }

    // Verify each task has correct status
    {
        use super::store::SchedulerStore;
        let user_store = SchedulerStore::new(user_db).expect("open user");
        let tasks = user_store.list_tasks_with_status().expect("list");

        assert_eq!(tasks.len(), 2);

        let task_1 = tasks
            .iter()
            .find(|t| t.id == task_id_1.to_string())
            .expect("find task 1");
        let task_2 = tasks
            .iter()
            .find(|t| t.id == task_id_2.to_string())
            .expect("find task 2");

        assert_eq!(task_1.execution_status, Some("success".to_string()));
        assert_eq!(task_2.execution_status, Some("failed".to_string()));
        assert!(task_1.error_message.is_none());
        assert_eq!(task_2.error_message, Some("timeout".to_string()));
    }
}

#[test]
fn run_task_failures_persist_retry_count_and_disable_at_limit() {
    let temp = TempDir::new().expect("tempdir");
    let tasks_db = temp.path().join("tasks.db");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");
    let run_task = base_run_task(&workspace, &mail_root);
    let quota_error = "ContainerGroupQuotaReached: container group quota reached";

    let mut scheduler =
        Scheduler::load(&tasks_db, FailingExecutor::new(quota_error)).expect("load scheduler");
    let task_id = scheduler
        .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))
        .expect("add run_task");
    assert!(scheduler.execute_task_by_id(task_id).is_err());

    let mut scheduler =
        Scheduler::load(&tasks_db, FailingExecutor::new(quota_error)).expect("reload scheduler");
    let first_retry_task = scheduler
        .tasks()
        .iter()
        .find(|task| task.id == task_id)
        .expect("task exists");
    let first_retry_at = match first_retry_task.schedule {
        Schedule::OneShot { run_at } => run_at,
        _ => panic!("expected one-shot schedule"),
    };
    assert!(first_retry_task.enabled);
    assert!(first_retry_at > Utc::now() + chrono::Duration::seconds(120));
    assert_eq!(
        scheduler
            .get_retry_count(&task_id.to_string())
            .expect("retry count"),
        1
    );

    force_one_shot_due(&mut scheduler, task_id);
    assert!(scheduler.execute_task_by_id(task_id).is_err());
    assert_eq!(
        scheduler
            .get_retry_count(&task_id.to_string())
            .expect("retry count"),
        2
    );

    force_one_shot_due(&mut scheduler, task_id);
    assert!(scheduler.execute_task_by_id(task_id).is_err());

    let scheduler =
        Scheduler::load(&tasks_db, FailingExecutor::new(quota_error)).expect("final reload");
    let final_task = scheduler
        .tasks()
        .iter()
        .find(|task| task.id == task_id)
        .expect("final task exists");
    assert!(!final_task.enabled);
    assert_eq!(
        scheduler
            .get_retry_count(&task_id.to_string())
            .expect("retry count"),
        0
    );
}

#[test]
fn run_task_channel_is_preserved_in_sync() {
    let temp = TempDir::new().expect("tempdir");
    let workspace_dir = temp.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create workspace");

    // Test Discord
    {
        let db = temp.path().join("discord_tasks.db");
        let run_task = discord_run_task(&workspace_dir);
        let mut scheduler = Scheduler::load(&db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))
            .expect("add");

        use super::store::SchedulerStore;
        let store = SchedulerStore::new(db).expect("open");
        let tasks = store.list_tasks_with_status().expect("list");
        assert_eq!(tasks[0].channel, "discord");
    }

    // Test Slack
    {
        let db = temp.path().join("slack_tasks.db");
        let run_task = slack_run_task(&workspace_dir);
        let mut scheduler = Scheduler::load(&db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))
            .expect("add");

        use super::store::SchedulerStore;
        let store = SchedulerStore::new(db).expect("open");
        let tasks = store.list_tasks_with_status().expect("list");
        assert_eq!(tasks[0].channel, "slack");
    }

    // Test Email (default)
    {
        let db = temp.path().join("email_tasks.db");
        let mail_root = temp.path().join("mail");
        fs::create_dir_all(&mail_root).expect("mail");
        let run_task = base_run_task(&workspace_dir, &mail_root);
        let mut scheduler = Scheduler::load(&db, NoopExecutor::default()).expect("load");
        scheduler
            .add_one_shot_in(Duration::from_secs(0), TaskKind::RunTask(run_task))
            .expect("add");

        use super::store::SchedulerStore;
        let store = SchedulerStore::new(db).expect("open");
        let tasks = store.list_tasks_with_status().expect("list");
        assert_eq!(tasks[0].channel, "email");
    }
}

#[test]
fn user_visible_routine_task_heuristic_is_conservative_for_one_shots() {
    let temp = TempDir::new().expect("tempdir");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");
    let run_task = base_run_task(&workspace, &mail_root);
    let now = Utc::now();

    let recurring = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task.clone()),
        schedule: Schedule::Cron {
            expression: "0 * * * * *".to_string(),
            next_run: now + chrono::Duration::minutes(30),
        },
        enabled: true,
        created_at: now,
        last_run: None,
    };
    assert!(
        is_user_visible_routine_task(&recurring, now),
        "cron run_task should always surface as a routine"
    );

    let future_one_shot = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task.clone()),
        schedule: Schedule::OneShot {
            run_at: now + chrono::Duration::minutes(2),
        },
        enabled: true,
        created_at: now,
        last_run: None,
    };
    assert!(
        is_user_visible_routine_task(&future_one_shot, now),
        "future one-shot run_task should surface as a routine"
    );

    let delayed_one_shot = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task.clone()),
        schedule: Schedule::OneShot {
            run_at: now + chrono::Duration::minutes(10),
        },
        enabled: false,
        created_at: now,
        last_run: Some(now + chrono::Duration::minutes(10)),
    };
    assert!(
        is_user_visible_routine_task(&delayed_one_shot, now + chrono::Duration::minutes(20)),
        "intentionally delayed one-shot should remain visible in history after it fires"
    );

    let ambiguous_one_shot = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task),
        schedule: Schedule::OneShot {
            run_at: now + chrono::Duration::minutes(2),
        },
        enabled: false,
        created_at: now,
        last_run: Some(now + chrono::Duration::minutes(2)),
    };
    assert!(
        !is_user_visible_routine_task(&ambiguous_one_shot, now + chrono::Duration::minutes(15)),
        "near-immediate one-shots should stay hidden once they are no longer future-scheduled"
    );
}

#[test]
fn user_visible_routine_task_excludes_non_run_task_kinds() {
    let now = Utc::now();
    let send_reply = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::Noop,
        schedule: Schedule::Cron {
            expression: "0 * * * * *".to_string(),
            next_run: now + chrono::Duration::minutes(5),
        },
        enabled: true,
        created_at: now,
        last_run: None,
    };

    assert!(
        !is_user_visible_routine_task(&send_reply, now),
        "only run_task items should be visible as routines"
    );
}

#[test]
fn prepare_task_for_resume_refreshes_schedule_safely() {
    let temp = TempDir::new().expect("tempdir");
    let workspace = temp.path().join("workspace");
    let mail_root = temp.path().join("mail");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(&mail_root).expect("mail");
    let run_task = base_run_task(&workspace, &mail_root);
    let now = Utc::now();

    let paused_cron = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task.clone()),
        schedule: Schedule::Cron {
            expression: "0 * * * * *".to_string(),
            next_run: now - chrono::Duration::minutes(30),
        },
        enabled: false,
        created_at: now - chrono::Duration::days(1),
        last_run: Some(now - chrono::Duration::hours(1)),
    };
    let resumed_cron = prepare_task_for_resume(&paused_cron, now).expect("resume cron");
    assert!(resumed_cron.enabled);
    match resumed_cron.schedule {
        Schedule::Cron { next_run, .. } => {
            assert!(
                next_run > now,
                "cron resume should refresh next_run into the future"
            );
        }
        _ => panic!("expected cron schedule"),
    }

    let future_one_shot = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task.clone()),
        schedule: Schedule::OneShot {
            run_at: now + chrono::Duration::hours(4),
        },
        enabled: false,
        created_at: now - chrono::Duration::hours(2),
        last_run: None,
    };
    let resumed_one_shot =
        prepare_task_for_resume(&future_one_shot, now).expect("resume future one-shot");
    assert!(resumed_one_shot.enabled);
    match resumed_one_shot.schedule {
        Schedule::OneShot { run_at } => {
            assert_eq!(run_at, now + chrono::Duration::hours(4));
        }
        _ => panic!("expected one-shot schedule"),
    }

    let past_one_shot = ScheduledTask {
        id: Uuid::new_v4(),
        kind: TaskKind::RunTask(run_task),
        schedule: Schedule::OneShot {
            run_at: now - chrono::Duration::minutes(1),
        },
        enabled: false,
        created_at: now - chrono::Duration::hours(3),
        last_run: Some(now - chrono::Duration::minutes(1)),
    };
    let error =
        prepare_task_for_resume(&past_one_shot, now).expect_err("past one-shot should fail");
    assert!(
        error.to_string().contains("run_at is in the past"),
        "resume should fail clearly for stale one-shot routines"
    );
}
