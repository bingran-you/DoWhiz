const appDir = process.env.PM2_APP_DIR || process.cwd();
const workerPort = process.env.RUST_SERVICE_PORT || "9001";
const killTimeout = Number(process.env.PM2_KILL_TIMEOUT_MS || 300000);
const listenTimeout = Number(process.env.PM2_LISTEN_TIMEOUT_MS || 15000);

module.exports = {
  apps: [
    {
      name: "dw_worker",
      cwd: appDir,
      script: "./target/release/rust_service",
      args: ["--host", "0.0.0.0", "--port", workerPort],
      interpreter: "none",
      kill_timeout: killTimeout,
      listen_timeout: listenTimeout,
    },
    {
      name: "dw_gateway",
      cwd: appDir,
      script: "./target/release/inbound_gateway",
      interpreter: "none",
      kill_timeout: killTimeout,
      listen_timeout: listenTimeout,
    },
  ],
};
