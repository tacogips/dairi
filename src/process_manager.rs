use thiserror::Error;

use bytes::BytesMut;
use once_cell::sync::OnceCell;
use regex::Regex;
use std::collections::HashMap;
use std::process::Stdio;

use sysinfo::{
    Pid, PidExt, Process, ProcessExt, ProcessRefreshKind, ProcessStatus, RefreshKind, System,
    SystemExt,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::select;
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::{self, timeout, Duration, Instant};

pub type CmdName = String;
type Input = String;
type Output = Vec<u8>;
const DEFAULT_CMD_TIMEOUT_SEC: u64 = 30;
const DEFAULT_WAIT_OUTPUT_FINISH_SEC: u64 = 2;

#[derive(Debug, Error)]
pub enum ProcessManagerError {
    #[error("CMD_TABLE has not initialiezd")]
    CmdTableNotInitialize,

    #[error("cmd not found. name:{0}")]
    CmdNotFound(CmdName),

    #[error("failed to get stdin of child process :{0}")]
    FailedToGetChildProcessStdin(CmdName),

    #[error("invalid regex :{0}")]
    RegexError(#[from] regex::Error),

    #[error("cmd failed with timeout")]
    Timeout(#[from] tokio::time::error::Elapsed),

    #[error("failed to add to process table :{0}")]
    FailedToAddProcessTable(CmdName),

    #[error("failed to get stdout of child process :{0}")]
    FailedToGetChildProcessStdout(CmdName),

    #[error("failed to get stderr of child process :{0}")]
    FailedToGetChildProcessStderr(CmdName),

    #[error("empty input not allowed")]
    EmptyInputNotAllowed,

    #[error("{0}")]
    IOError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ProcessManagerError>;

pub struct RunningProcess {
    running_cmd: &'static Cmd,
    child: Child,
}

#[derive(Debug)]
pub struct Cmd {
    pub name: CmdName,
    pub cmd: String,
    pub output_size: usize,
    pub auto_trailing_newline: bool,
    pub join_input_newline_with: Option<String>,
    pub truncate_line_regex: Option<String>,
    pub remove_empty_line: bool,
    pub no_empty_input: bool,
    pub timeout_sec: Option<u64>,
    pub wait_output_timeout_milli_sec: Option<u64>,
}

impl Cmd {
    pub fn new(
        name: CmdName,
        cmd: String,
        output_size: usize,
        auto_trailing_newline: bool,
        join_input_newline_with: Option<String>,
        truncate_line_regex: Option<String>,
        remove_empty_line: bool,
        no_empty_input: bool,
        timeout_sec: Option<u64>,
        wait_output_timeout_milli_sec: Option<u64>,
    ) -> Self {
        Self {
            name,
            cmd,
            output_size,
            auto_trailing_newline,
            join_input_newline_with,
            truncate_line_regex,
            remove_empty_line,
            no_empty_input,
            timeout_sec,
            wait_output_timeout_milli_sec,
        }
    }
}

pub type CmdTable = HashMap<CmdName, Cmd>;
static CMD_TABLE: OnceCell<CmdTable> = OnceCell::new();

type ProcessTable = HashMap<CmdName, RunningProcess>;
static PROCESS_TABLE: OnceCell<Mutex<ProcessTable>> = OnceCell::new();

pub fn init_cmd_table(
    cmd_table: HashMap<CmdName, Cmd>,
) -> std::result::Result<(), HashMap<CmdName, Cmd>> {
    CMD_TABLE.set(cmd_table)
}

fn get_cmd_from_table(cmd_name: &CmdName) -> Result<&'static Cmd> {
    let cmd_table = CMD_TABLE
        .get()
        .ok_or_else(|| ProcessManagerError::CmdTableNotInitialize)?;

    cmd_table
        .get(cmd_name)
        .ok_or_else(|| ProcessManagerError::CmdNotFound(cmd_name.clone()))
}

fn process_table() -> &'static Mutex<ProcessTable> {
    PROCESS_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn add_to_process_table(
    process_table: &mut MutexGuard<ProcessTable>,
    running_process: RunningProcess,
) -> Result<()> {
    let cmd_name = running_process.running_cmd.cmd.clone();
    process_table.insert(cmd_name, running_process);
    Ok(())
}

fn is_health_process(p: &Process) -> bool {
    match p.status() {
        ProcessStatus::Run
        | ProcessStatus::Idle
        | ProcessStatus::Sleep
        | ProcessStatus::Tracing => true,
        _ => false,
    }
}

pub async fn run_cmd(name: &CmdName, input: Input, output_size: Option<usize>) -> Result<Output> {
    // TODO(tacogips) TOBE run concurrently. this mutex hold the lock until the process ends
    let mut proceses = process_table().lock().await;
    if let Some(running_process) = proceses.get_mut(name) {
        if let Some(pid) = running_process.child.id() {
            let target_pid = Pid::from_u32(pid);

            let refresh_kind = RefreshKind::new();
            let refresh_kind = refresh_kind.with_processes(ProcessRefreshKind::everything());
            let sys = System::new_with_specifics(refresh_kind);
            if let Some(os_process) = sys.process(target_pid) {
                if is_health_process(os_process) {
                    tracing::debug!("run existing process {}, {}", name, input);

                    let timeout_sec = running_process
                        .running_cmd
                        .timeout_sec
                        .unwrap_or(DEFAULT_CMD_TIMEOUT_SEC);
                    return timeout(
                        Duration::from_secs(timeout_sec),
                        pass_input_to_process(
                            name,
                            &mut running_process.child,
                            input,
                            output_size.unwrap_or(running_process.running_cmd.output_size),
                            running_process.running_cmd.auto_trailing_newline,
                            running_process.running_cmd.join_input_newline_with.as_ref(),
                            running_process.running_cmd.truncate_line_regex.as_ref(),
                            running_process.running_cmd.remove_empty_line,
                            running_process.running_cmd.no_empty_input,
                            running_process.running_cmd.wait_output_timeout_milli_sec,
                        ),
                    )
                    .await?;
                } else {
                    // kill zomibie process
                    os_process.kill();
                }
            }
        }
    };

    tracing::debug!("spawn process: {}", name);
    let spawned_process = spawn_process(name).await?;
    add_to_process_table(&mut proceses, spawned_process)?;
    tracing::debug!("process spawend: {}", name);

    match proceses.get_mut(name) {
        Some(p) => {
            let timeout_sec = p.running_cmd.timeout_sec.unwrap_or(DEFAULT_CMD_TIMEOUT_SEC);

            let output = timeout(
                Duration::from_secs(timeout_sec),
                pass_input_to_process(
                    name,
                    &mut p.child,
                    input,
                    output_size.unwrap_or(p.running_cmd.output_size),
                    p.running_cmd.auto_trailing_newline,
                    p.running_cmd.join_input_newline_with.as_ref(),
                    p.running_cmd.truncate_line_regex.as_ref(),
                    p.running_cmd.remove_empty_line,
                    p.running_cmd.no_empty_input,
                    p.running_cmd.wait_output_timeout_milli_sec,
                ),
            )
            .await??;

            tracing::debug!("input passed the process: {}", name);
            Ok(output)
        }
        None => Err(ProcessManagerError::FailedToAddProcessTable(name.clone())),
    }
}

fn arrange_input(
    mut input: String,
    auto_trailing_newline: bool,
    join_new_lines_with: Option<&String>,
    truncate_line_regex: Option<&String>,
    remove_empty_line: bool,
) -> Result<String> {
    if let Some(truncate_line_regex) = truncate_line_regex {
        let re = Regex::new(truncate_line_regex)?;
        let mut ss = Vec::<String>::new();
        for each in input.split("\n") {
            ss.push(re.replace_all(each, "").to_string());
        }
        input = ss.join("\n")
    }
    if remove_empty_line {
        let empty_line_regex: Regex = Regex::new(r"^[\s\t]+$").unwrap();
        let mut ss = Vec::<String>::new();
        for each in input.split("\n") {
            if !each.is_empty() && !empty_line_regex.is_match(each) {
                ss.push(each.to_string());
            }
        }
        input = ss.join("\n")
    }

    //TODO(tacogips) retain the trailing new line
    if let Some(rep) = join_new_lines_with {
        input = input.replace("\n", &rep);
    }
    if auto_trailing_newline {
        input = format!("{}\n", input);
    }

    Ok(input)
}

async fn pass_input_to_process(
    name: &CmdName,
    child: &mut Child,
    input: Input,
    max_output_size: usize,
    auto_trailing_newline: bool,
    join_input_new_lines_with: Option<&String>,
    truncate_line_regex: Option<&String>,
    remove_empty_line: bool,
    no_empty_input: bool,
    wait_output_timeout_milli_sec: Option<u64>,
) -> Result<Output> {
    let input = arrange_input(
        input,
        auto_trailing_newline,
        join_input_new_lines_with,
        truncate_line_regex,
        remove_empty_line,
    )?;
    if no_empty_input {
        if input.is_empty() || Regex::new(r"^[\s\n]+$")?.is_match(&input) {
            return Err(ProcessManagerError::EmptyInputNotAllowed);
        }
    }

    tracing::info!("cmd:{}, input:  {}", name, input);
    let child_stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| ProcessManagerError::FailedToGetChildProcessStdin(name.clone()))?;

    let child_stdout = child
        .stdout
        .as_mut()
        .ok_or_else(|| ProcessManagerError::FailedToGetChildProcessStdout(name.clone()))?;

    let child_stderr = child
        .stderr
        .as_mut()
        .ok_or_else(|| ProcessManagerError::FailedToGetChildProcessStderr(name.clone()))?;

    tracing::debug!(" passing to stdin of process :{} {}", name, input);

    child_stdin.write_all(input.as_bytes()).await?;
    tracing::debug!(" reading from stdout of process :{}", name);

    let mut std_out_read_buf = BytesMut::with_capacity(max_output_size);
    let mut std_out_reader = BufReader::with_capacity(max_output_size, child_stdout);

    let mut std_err_read_buf = BytesMut::with_capacity(max_output_size);
    let mut std_err_reader = BufReader::with_capacity(max_output_size, child_stderr);

    let latest_read_at: Mutex<Option<Instant>> = Mutex::new(None);
    let mut result = Output::new();

    let wait_duration_sequential_output = Duration::from_millis(
        wait_output_timeout_milli_sec.unwrap_or(DEFAULT_WAIT_OUTPUT_FINISH_SEC),
    );
    let mut check_output_finished_interval = time::interval(Duration::from_millis(100));

    // wait output ends during `wait_duration_sequential_output` seconds elapsed
    loop {
        select! {
            std_out = std_out_reader.read_buf(&mut std_out_read_buf) => {
                match std_out {
                    Err(e) => {
                        tracing::debug!(" read stdout error :{}", e);
                        return Err(ProcessManagerError::IOError(e))
                    }
                    Ok(read_size) => {
                        tracing::debug!(
                            " finished to read from stdout of process :{:?}",
                            String::from_utf8(std_out_read_buf[..read_size].to_vec())
                        );

                        result.append(&mut std_out_read_buf[..read_size].to_vec());
                        std_out_read_buf.clear();

                        let mut read_at =  latest_read_at.lock().await;
                        read_at.replace(Instant::now());
                        drop(read_at);
                        continue
                    }
                }
            }

            std_err = std_err_reader.read_buf(&mut std_err_read_buf) => {
                match std_err {
                    Err(e) => {
                        tracing::debug!(" read stdout error :{}", e);
                        return Err(ProcessManagerError::IOError(e))
                    }
                    Ok(read_size) => {
                        tracing::debug!(
                            " finished to read from stdout of process :{:?}",
                            String::from_utf8(std_err_read_buf[..read_size].to_vec())
                        );
                        result.append(&mut std_err_read_buf[..read_size].to_vec());
                        std_err_read_buf.clear();

                        let mut read_at =  latest_read_at.lock().await;
                        read_at.replace(Instant::now());
                        drop(read_at);
                        continue
                    }
                }
            }

            check_at = check_output_finished_interval.tick() => {
                let read_at =  latest_read_at.lock().await;
                if let Some(latest_read_at) = *read_at {
                    let duration_since_checked = check_at.duration_since(latest_read_at);
                    if duration_since_checked  >= wait_duration_sequential_output {
                        break
                    }
                }

            }
        }
    }
    Ok(result)
}

async fn spawn_process(name: &CmdName) -> Result<RunningProcess> {
    let cmd: &'static Cmd = get_cmd_from_table(name)?;
    let child = Command::new(cmd.cmd.clone())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let running_process = RunningProcess {
        running_cmd: cmd,
        child,
    };

    Ok(running_process)
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_arrange_input() {
        {
            let input = arrange_input(
                "aaa\nbbb".to_string(),
                false,
                Some(&";".to_string()),
                None,
                false,
            );
            assert_eq!("aaa;bbb".to_string(), input.unwrap());
        }

        {
            let input = arrange_input(
                "
                # sss
                aaa # ddd
 \n\nbbb"
                    .to_string(),
                true,
                Some(&";".to_string()),
                Some(&"#.*".to_string()),
                false,
            );
            assert_eq!(
                ";                ;                aaa ; ;;bbb\n".to_string(),
                input.unwrap()
            );
        }

        {
            let input = arrange_input(
                "
                # sss
                aaa # ddd
 \n\nbbb"
                    .to_string(),
                true,
                Some(&";".to_string()),
                Some(&"#.*".to_string()),
                true,
            );
            assert_eq!("                aaa ;bbb\n".to_string(), input.unwrap());
        }
    }
}
