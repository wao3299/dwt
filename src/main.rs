use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::process::Command;

const DEFAULT_ALLOWED: &str = "git,uv,python,npm";

struct Config {
    container: String,
    workdir: String,
    allowed: Vec<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = match load_config().and_then(|cfg| parse(&cfg, &args)) {
        Ok((workdir, container, cmd)) => {
            let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            Command::new("docker")
                .arg("exec")
                .arg(if tty { "-it" } else { "-i" })
                .arg("-w")
                .arg(workdir)
                .arg(container)
                .args(cmd)
                .exec()
        }
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    eprintln!("dwt: docker exec の起動に失敗: {err}");
    std::process::exit(1);
}

fn load_config() -> Result<Config, String> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
    let path = format!("{base}/dwt/config");
    let mut container = None;
    let mut workdir = None;
    let mut allowed = DEFAULT_ALLOWED.to_string();
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                match key.trim() {
                    "container" => container = Some(value.trim().to_string()),
                    "workdir" => workdir = Some(value.trim().to_string()),
                    "allowed" => allowed = value.trim().to_string(),
                    _ => {}
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("dwt: 設定ファイル {path} を読めない: {e}")),
    }
    if let Ok(v) = std::env::var("DWT_CONTAINER") {
        container = Some(v);
    }
    if let Ok(v) = std::env::var("DWT_WORKDIR") {
        workdir = Some(v);
    }
    if let Ok(v) = std::env::var("DWT_ALLOWED") {
        allowed = v;
    }
    let missing =
        |key: &str, var: &str| format!("dwt: {key} が未設定 (環境変数 {var} か {path} で指定する)");
    Ok(Config {
        container: container.ok_or_else(|| missing("container", "DWT_CONTAINER"))?,
        workdir: workdir.ok_or_else(|| missing("workdir", "DWT_WORKDIR"))?,
        allowed: allowed.split(',').map(|s| s.trim().to_string()).collect(),
    })
}

fn parse<'a>(
    cfg: &Config,
    args: &'a [String],
) -> Result<(String, String, &'a [String]), String> {
    const USAGE: &str = "usage: dwt <N> <command> [args...]";
    let n: u32 = args
        .first()
        .ok_or(USAGE)?
        .parse()
        .map_err(|_| USAGE.to_string())?;
    let cmd = &args[1..];
    let head = cmd.first().ok_or(USAGE)?;
    if !cfg.allowed.iter().any(|a| a == head) {
        return Err(format!(
            "dwt: '{head}' は許可コマンド ({}) に含まれない。素の docker exec を使うこと",
            cfg.allowed.join(", ")
        ));
    }
    if head == "git" {
        let subcommand = cmd[1..].iter().find(|a| !a.starts_with('-'));
        if subcommand.is_some_and(|s| s == "push") {
            return Err("dwt: git push は dwt では実行できない。素の docker exec を使うこと".into());
        }
        if cmd[1..].iter().any(|a| a.starts_with("--force")) {
            return Err(
                "dwt: --force 系オプションは dwt では実行できない。素の docker exec を使うこと"
                    .into(),
            );
        }
    }
    let n = n.to_string();
    Ok((
        cfg.workdir.replace("{n}", &n),
        cfg.container.replace("{n}", &n),
        cmd,
    ))
}
