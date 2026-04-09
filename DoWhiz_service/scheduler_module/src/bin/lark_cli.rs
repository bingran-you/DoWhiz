use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use serde_json::json;

const LARK_BASE_URL: &str = "https://open.larksuite.com";

#[derive(Parser)]
#[command(name = "lark_cli")]
#[command(about = "Lark API CLI for Docs, Sheets, Bitable, and Drive")]
struct Cli {
    #[arg(long, env = "LARK_APP_ID")]
    app_id: String,

    #[arg(long, env = "LARK_APP_SECRET")]
    app_secret: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    GetDoc {
        #[arg(long)]
        document_id: String,
    },
    ReadDoc {
        #[arg(long)]
        document_id: String,
    },
    CreateDoc {
        #[arg(long)]
        title: String,
        #[arg(long)]
        folder_token: Option<String>,
    },

    GetSheet {
        #[arg(long)]
        spreadsheet_id: String,
    },
    ReadRange {
        #[arg(long)]
        spreadsheet_id: String,
        #[arg(long)]
        sheet_id: String,
        #[arg(long)]
        range: String,
    },
    WriteRange {
        #[arg(long)]
        spreadsheet_id: String,
        #[arg(long)]
        sheet_id: String,
        #[arg(long)]
        range: String,
        #[arg(long)]
        values: String,
    },
    AppendRows {
        #[arg(long)]
        spreadsheet_id: String,
        #[arg(long)]
        sheet_id: String,
        #[arg(long)]
        values: String,
    },

    ListTables {
        #[arg(long)]
        app_token: String,
    },
    GetTable {
        #[arg(long)]
        app_token: String,
        #[arg(long)]
        table_id: String,
    },
    QueryRecords {
        #[arg(long)]
        app_token: String,
        #[arg(long)]
        table_id: String,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value = "20")]
        page_size: u32,
    },
    CreateRecord {
        #[arg(long)]
        app_token: String,
        #[arg(long)]
        table_id: String,
        #[arg(long)]
        fields: String,
    },
    UpdateRecord {
        #[arg(long)]
        app_token: String,
        #[arg(long)]
        table_id: String,
        #[arg(long)]
        record_id: String,
        #[arg(long)]
        fields: String,
    },
    DeleteRecord {
        #[arg(long)]
        app_token: String,
        #[arg(long)]
        table_id: String,
        #[arg(long)]
        record_id: String,
    },

    ListFiles {
        #[arg(long)]
        folder_token: Option<String>,
        #[arg(long, default_value = "20")]
        page_size: u32,
    },
    GetFile {
        #[arg(long)]
        file_token: String,
    },
    CreateFolder {
        #[arg(long)]
        name: String,
        #[arg(long)]
        parent_token: String,
    },
    ShareFile {
        #[arg(long)]
        token: String,
        #[arg(long, help = "File type: docx, sheet, bitable, file, folder")]
        file_type: String,
        #[arg(
            long,
            default_value = "openid",
            help = "Member type: openid, email, userid"
        )]
        member_type: String,
        #[arg(long, help = "Member ID (open_id like ou_xxx, email, or user_id)")]
        member_id: String,
        #[arg(
            long,
            default_value = "edit",
            help = "Permission: view, edit, full_access"
        )]
        perm: String,
    },
}

async fn get_tenant_access_token(app_id: &str, app_secret: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            LARK_BASE_URL
        ))
        .json(&json!({
            "app_id": app_id,
            "app_secret": app_secret
        }))
        .send()
        .await?;

    let status = resp.status();
    let data: serde_json::Value = resp.json().await?;

    if !status.is_success() || data["code"].as_i64().unwrap_or(-1) != 0 {
        bail!("Failed to get token: {}", data);
    }

    data["tenant_access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Missing tenant_access_token in response"))
}

async fn get_doc(token: &str, document_id: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/open-apis/docx/v1/documents/{}",
            LARK_BASE_URL, document_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn read_doc(token: &str, document_id: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/open-apis/docx/v1/documents/{}/blocks",
            LARK_BASE_URL, document_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn create_doc(token: &str, title: &str, folder_token: Option<&str>) -> Result<()> {
    let mut body = json!({ "title": title });
    if let Some(folder) = folder_token {
        body["folder_token"] = json!(folder);
    }

    let resp = reqwest::Client::new()
        .post(format!("{}/open-apis/docx/v1/documents", LARK_BASE_URL))
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn get_sheet(token: &str, spreadsheet_id: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/open-apis/sheets/v3/spreadsheets/{}",
            LARK_BASE_URL, spreadsheet_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn read_range(token: &str, spreadsheet_id: &str, sheet_id: &str, range: &str) -> Result<()> {
    let full_range = format!("{}!{}", sheet_id, range);
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/open-apis/sheets/v2/spreadsheets/{}/values/{}",
            LARK_BASE_URL, spreadsheet_id, full_range
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn write_range(
    token: &str,
    spreadsheet_id: &str,
    sheet_id: &str,
    range: &str,
    values: &str,
) -> Result<()> {
    let full_range = format!("{}!{}", sheet_id, range);
    let values: serde_json::Value = serde_json::from_str(values)?;

    let resp = reqwest::Client::new()
        .put(format!(
            "{}/open-apis/sheets/v2/spreadsheets/{}/values",
            LARK_BASE_URL, spreadsheet_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "valueRange": {
                "range": full_range,
                "values": values
            }
        }))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn append_rows(
    token: &str,
    spreadsheet_id: &str,
    sheet_id: &str,
    values: &str,
) -> Result<()> {
    let values: serde_json::Value = serde_json::from_str(values)?;

    let resp = reqwest::Client::new()
        .post(format!(
            "{}/open-apis/sheets/v2/spreadsheets/{}/values_append",
            LARK_BASE_URL, spreadsheet_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "valueRange": {
                "range": sheet_id,
                "values": values
            }
        }))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn list_tables(token: &str, app_token: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/open-apis/bitable/v1/apps/{}/tables",
            LARK_BASE_URL, app_token
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn get_table(token: &str, app_token: &str, table_id: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}",
            LARK_BASE_URL, app_token, table_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn query_records(
    token: &str,
    app_token: &str,
    table_id: &str,
    filter: Option<&str>,
    page_size: u32,
) -> Result<()> {
    let mut body = json!({ "page_size": page_size });
    if let Some(f) = filter {
        body["filter"] = json!(f);
    }

    let resp = reqwest::Client::new()
        .post(format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records/search",
            LARK_BASE_URL, app_token, table_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn create_record(token: &str, app_token: &str, table_id: &str, fields: &str) -> Result<()> {
    let fields: serde_json::Value = serde_json::from_str(fields)?;

    let resp = reqwest::Client::new()
        .post(format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records",
            LARK_BASE_URL, app_token, table_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({ "fields": fields }))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn update_record(
    token: &str,
    app_token: &str,
    table_id: &str,
    record_id: &str,
    fields: &str,
) -> Result<()> {
    let fields: serde_json::Value = serde_json::from_str(fields)?;

    let resp = reqwest::Client::new()
        .put(format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records/{}",
            LARK_BASE_URL, app_token, table_id, record_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({ "fields": fields }))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn delete_record(
    token: &str,
    app_token: &str,
    table_id: &str,
    record_id: &str,
) -> Result<()> {
    let resp = reqwest::Client::new()
        .delete(format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records/{}",
            LARK_BASE_URL, app_token, table_id, record_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn list_files(token: &str, folder_token: Option<&str>, page_size: u32) -> Result<()> {
    let mut url = format!(
        "{}/open-apis/drive/v1/files?page_size={}",
        LARK_BASE_URL, page_size
    );
    if let Some(folder) = folder_token {
        url.push_str(&format!("&folder_token={}", folder));
    }

    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn get_file(token: &str, file_token: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/open-apis/drive/v1/files/{}",
            LARK_BASE_URL, file_token
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn create_folder(token: &str, name: &str, parent_token: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/open-apis/drive/v1/files/create_folder",
            LARK_BASE_URL
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "name": name,
            "folder_token": parent_token
        }))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

async fn share_file(
    token: &str,
    file_token: &str,
    file_type: &str,
    member_type: &str,
    member_id: &str,
    perm: &str,
) -> Result<()> {
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/open-apis/drive/v1/permissions/{}/members?type={}",
            LARK_BASE_URL, file_token, file_type
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "member_type": member_type,
            "member_id": member_id,
            "perm": perm
        }))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let token = get_tenant_access_token(&cli.app_id, &cli.app_secret).await?;

    match &cli.command {
        Commands::GetDoc { document_id } => get_doc(&token, document_id).await?,
        Commands::ReadDoc { document_id } => read_doc(&token, document_id).await?,
        Commands::CreateDoc {
            title,
            folder_token,
        } => create_doc(&token, title, folder_token.as_deref()).await?,

        Commands::GetSheet { spreadsheet_id } => get_sheet(&token, spreadsheet_id).await?,
        Commands::ReadRange {
            spreadsheet_id,
            sheet_id,
            range,
        } => read_range(&token, spreadsheet_id, sheet_id, range).await?,
        Commands::WriteRange {
            spreadsheet_id,
            sheet_id,
            range,
            values,
        } => write_range(&token, spreadsheet_id, sheet_id, range, values).await?,
        Commands::AppendRows {
            spreadsheet_id,
            sheet_id,
            values,
        } => append_rows(&token, spreadsheet_id, sheet_id, values).await?,

        Commands::ListTables { app_token } => list_tables(&token, app_token).await?,
        Commands::GetTable {
            app_token,
            table_id,
        } => get_table(&token, app_token, table_id).await?,
        Commands::QueryRecords {
            app_token,
            table_id,
            filter,
            page_size,
        } => query_records(&token, app_token, table_id, filter.as_deref(), *page_size).await?,
        Commands::CreateRecord {
            app_token,
            table_id,
            fields,
        } => create_record(&token, app_token, table_id, fields).await?,
        Commands::UpdateRecord {
            app_token,
            table_id,
            record_id,
            fields,
        } => update_record(&token, app_token, table_id, record_id, fields).await?,
        Commands::DeleteRecord {
            app_token,
            table_id,
            record_id,
        } => delete_record(&token, app_token, table_id, record_id).await?,

        Commands::ListFiles {
            folder_token,
            page_size,
        } => list_files(&token, folder_token.as_deref(), *page_size).await?,
        Commands::GetFile { file_token } => get_file(&token, file_token).await?,
        Commands::CreateFolder { name, parent_token } => {
            create_folder(&token, name, parent_token).await?
        }
        Commands::ShareFile {
            token: file_token,
            file_type,
            member_type,
            member_id,
            perm,
        } => share_file(&token, file_token, file_type, member_type, member_id, perm).await?,
    }

    Ok(())
}
