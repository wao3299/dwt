use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::process::Command;

const DEFAULT_ALLOWED: &str = "git,uv,python,npm";

struct Config {
    container: String,
    workdir: String,
    allowed: Vec<String>,
    path_map: Option<(String, String)>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = match load_config().and_then(|cfg| parse(&cfg, &args)) {
        Ok((workdir, container, cmd, path_map)) => {
            let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            let mut docker = Command::new("docker");
            docker
                .arg("exec")
                .arg(if tty { "-it" } else { "-i" })
                .arg("-w")
                .arg(&workdir);
            if let Some((host, inner)) = &path_map {
                for (key, value) in git_env(host, inner, &workdir) {
                    docker.arg("-e").arg(format!("{key}={value}"));
                }
            }
            docker.arg(container).args(cmd).exec()
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
    let mut path_map = None;
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
                    "path_map" => path_map = Some(value.trim().to_string()),
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
    if let Ok(v) = std::env::var("DWT_PATH_MAP") {
        path_map = Some(v);
    }
    let path_map = match path_map {
        None => None,
        Some(v) => Some(parse_path_map(&v).ok_or_else(|| {
            format!("dwt: path_map は <ホスト側パス>=<コンテナ側パス> の形式で指定する: {v}")
        })?),
    };
    let missing =
        |key: &str, var: &str| format!("dwt: {key} が未設定 (環境変数 {var} か {path} で指定する)");
    Ok(Config {
        container: container.ok_or_else(|| missing("container", "DWT_CONTAINER"))?,
        workdir: workdir.ok_or_else(|| missing("workdir", "DWT_WORKDIR"))?,
        allowed: allowed.split(',').map(|s| s.trim().to_string()).collect(),
        path_map,
    })
}

fn parse_path_map(value: &str) -> Option<(String, String)> {
    let (host, inner) = value.split_once('=')?;
    let host = host.trim().trim_end_matches('/');
    let inner = inner.trim().trim_end_matches('/');
    if host.is_empty() || inner.is_empty() {
        return None;
    }
    Some((host.to_string(), inner.to_string()))
}

fn translate(path: &str, from: &str, to: &str) -> Option<String> {
    let rest = path.strip_prefix(from)?;
    if !rest.is_empty() && !rest.starts_with('/') {
        return None;
    }
    Some(format!("{to}{rest}"))
}

/// ホスト側で作られた worktree は `.git` の gitdir がホスト側パスを指し、コンテナ内の git が
/// 解決できない。その場合だけ、コンテナ側パスに読み替えた GIT_DIR / GIT_WORK_TREE を返す。
fn git_env(host: &str, inner: &str, workdir: &str) -> Vec<(&'static str, String)> {
    let Some(host_workdir) = translate(workdir, inner, host) else {
        return vec![];
    };
    let Ok(text) = std::fs::read_to_string(format!("{host_workdir}/.git")) else {
        return vec![];
    };
    gitdir_env(&text, host, inner, workdir)
}

fn gitdir_env(
    dot_git: &str,
    host: &str,
    inner: &str,
    workdir: &str,
) -> Vec<(&'static str, String)> {
    let Some(gitdir) = dot_git.lines().find_map(|l| l.strip_prefix("gitdir:")) else {
        return vec![];
    };
    match translate(gitdir.trim(), host, inner) {
        Some(gitdir) => vec![("GIT_DIR", gitdir), ("GIT_WORK_TREE", workdir.to_string())],
        None => vec![],
    }
}

type Parsed<'a> = (String, String, &'a [String], Option<(String, String)>);

fn parse<'a>(cfg: &Config, args: &'a [String]) -> Result<Parsed<'a>, String> {
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
            return Err(
                "dwt: git push は dwt では実行できない。素の docker exec を使うこと".into(),
            );
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
        cfg.path_map.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "/Users/me/proj";
    const INNER: &str = "/workspaces";

    #[test]
    fn translate_は境界でだけ一致する() {
        assert_eq!(
            translate("/Users/me/proj/worktree-1", HOST, INNER).as_deref(),
            Some("/workspaces/worktree-1")
        );
        assert_eq!(
            translate("/Users/me/proj", HOST, INNER).as_deref(),
            Some("/workspaces")
        );
        assert_eq!(translate("/Users/me/proj2/worktree-1", HOST, INNER), None);
    }

    #[test]
    fn ホスト側を指す_gitdir_はコンテナ側に読み替える() {
        let env = gitdir_env(
            "gitdir: /Users/me/proj/app/.git/worktrees/worktree-1\n",
            HOST,
            INNER,
            "/workspaces/worktree-1",
        );
        assert_eq!(
            env,
            vec![
                (
                    "GIT_DIR",
                    "/workspaces/app/.git/worktrees/worktree-1".to_string()
                ),
                ("GIT_WORK_TREE", "/workspaces/worktree-1".to_string()),
            ]
        );
    }

    #[test]
    fn コンテナ側を指す_gitdir_は何も足さない() {
        let env = gitdir_env(
            "gitdir: /workspaces/app/.git/worktrees/worktree-1\n",
            HOST,
            INNER,
            "/workspaces/worktree-1",
        );
        assert!(env.is_empty());
    }

    #[test]
    fn path_map_の形式() {
        assert_eq!(
            parse_path_map("/Users/me/proj/ = /workspaces/"),
            Some((HOST.to_string(), INNER.to_string()))
        );
        assert_eq!(parse_path_map("/Users/me/proj"), None);
        assert_eq!(parse_path_map("=/workspaces"), None);
    }
}
