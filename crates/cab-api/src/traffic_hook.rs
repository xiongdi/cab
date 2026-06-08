//! LD_PRELOAD traffic hijack — redirect selected hostnames to a local CAB port.
//!
//! Used by agent **proxy mode** when the client does not support custom base URLs
//! (e.g. Antigravity CLI → `daily-cloudcode-pa.googleapis.com`).
//!
//! Go clients (agy) resolve hostnames via libc but dial URL port 443 explicitly,
//! so we also hook `connect()` to remap loopback:443 → CAB HTTPS port.

use cab_core::CabError;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct TrafficHookPaths {
    pub source: PathBuf,
    pub library: PathBuf,
}

pub fn hook_paths(base_dir: &Path) -> TrafficHookPaths {
    TrafficHookPaths {
        source: base_dir.join("hook_connect.c"),
        library: base_dir.join("libcab_hook.so"),
    }
}

/// Merge system CA store with CAB's self-signed cert so Go/agy trusts the local TLS terminator.
pub fn ensure_ca_bundle(cab_dir: &Path, gemini_dir: &Path) -> Result<PathBuf, CabError> {
    fs::create_dir_all(cab_dir).map_err(|e| CabError::NotFound(e.to_string()))?;
    let bundle = cab_dir.join("cab-ca-bundle.crt");
    let mut content = String::new();

    for system_path in [
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
    ] {
        let path = Path::new(system_path);
        if path.exists() {
            content.push_str(
                &fs::read_to_string(path).map_err(|e| CabError::NotFound(e.to_string()))?,
            );
            break;
        }
    }

    let cab_cert = gemini_dir.join("cert.pem");
    if cab_cert.exists() {
        content.push('\n');
        content.push_str(
            &fs::read_to_string(&cab_cert).map_err(|e| CabError::NotFound(e.to_string()))?,
        );
    }

    if content.is_empty() {
        return Err(CabError::NotFound(
            "could not build CA bundle: no system or CAB certificates found".into(),
        ));
    }

    fs::write(&bundle, content).map_err(|e| CabError::NotFound(e.to_string()))?;
    Ok(bundle)
}

/// Compile a shared library that overrides `getaddrinfo` + `connect` for the given hostnames.
pub fn compile_traffic_hook(
    hosts: &[&str],
    redirect_port: u16,
    paths: &TrafficHookPaths,
) -> Result<(), CabError> {
    if hosts.is_empty() {
        return Err(CabError::InvalidRequest(
            "traffic hook requires at least one hostname".into(),
        ));
    }

    if let Some(parent) = paths.source.parent() {
        fs::create_dir_all(parent).map_err(|e| CabError::NotFound(e.to_string()))?;
    }

    let host_checks = hosts
        .iter()
        .map(|host| {
            format!(
                r#"    if (strcmp(node, "{host}") == 0) {{
        return cab_redirect_local(service, hints, res);
    }}"#,
                host = host
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let c_code = format!(
        r#"#define _GNU_SOURCE
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <unistd.h>

static unsigned short cab_proxy_port = {default_port};

int (*real_getaddrinfo)(const char *node, const char *service,
                        const struct addrinfo *hints,
                        struct addrinfo **res) = NULL;
int (*real_connect)(int sockfd, const struct sockaddr *addr, socklen_t addrlen) = NULL;

__attribute__((constructor))
static void cab_init(void) {{
    const char *p = getenv("CAB_PROXY_PORT");
    if (p && *p) {{
        int v = atoi(p);
        if (v > 0 && v <= 65535) cab_proxy_port = (unsigned short)v;
    }}
}}

static int cab_redirect_local(const char *service, const struct addrinfo *hints,
                              struct addrinfo **res) {{
    int ret = real_getaddrinfo("127.0.0.1", service, hints, res);
    if (ret == 0 && res && *res) {{
        struct addrinfo *curr = *res;
        while (curr) {{
            if (curr->ai_family == AF_INET) {{
                struct sockaddr_in *addr_in = (struct sockaddr_in *)curr->ai_addr;
                addr_in->sin_port = htons(cab_proxy_port);
            }} else if (curr->ai_family == AF_INET6) {{
                struct sockaddr_in6 *addr_in6 = (struct sockaddr_in6 *)curr->ai_addr;
                addr_in6->sin6_port = htons(cab_proxy_port);
            }}
            curr = curr->ai_next;
        }}
    }}
    return ret;
}}

static int cab_remap_loopback_https(const struct sockaddr *addr,
                                    struct sockaddr_storage *out,
                                    socklen_t *out_len) {{
    if (!addr) return 0;

    if (addr->sa_family == AF_INET) {{
        const struct sockaddr_in *sin = (const struct sockaddr_in *)addr;
        if (sin->sin_addr.s_addr == htonl(0x7f000001) && ntohs(sin->sin_port) == 443) {{
            struct sockaddr_in *mod = (struct sockaddr_in *)out;
            *mod = *sin;
            mod->sin_port = htons(cab_proxy_port);
            *out_len = sizeof(struct sockaddr_in);
            return 1;
        }}
    }} else if (addr->sa_family == AF_INET6) {{
        const struct sockaddr_in6 *sin6 = (const struct sockaddr_in6 *)addr;
        struct in6_addr loopback6 = IN6ADDR_LOOPBACK_INIT;
        if (memcmp(&sin6->sin6_addr, &loopback6, sizeof(loopback6)) == 0
            && ntohs(sin6->sin6_port) == 443) {{
            struct sockaddr_in6 *mod = (struct sockaddr_in6 *)out;
            *mod = *sin6;
            mod->sin6_port = htons(cab_proxy_port);
            *out_len = sizeof(struct sockaddr_in6);
            return 1;
        }}
    }}
    return 0;
}}

int getaddrinfo(const char *node, const char *service,
                const struct addrinfo *hints,
                struct addrinfo **res) {{
    if (!real_getaddrinfo) {{
        real_getaddrinfo = dlsym(RTLD_NEXT, "getaddrinfo");
    }}

    if (node) {{
{host_checks}
    }}

    return real_getaddrinfo(node, service, hints, res);
}}

int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {{
    if (!real_connect) {{
        real_connect = dlsym(RTLD_NEXT, "connect");
    }}

    struct sockaddr_storage storage;
    socklen_t len = addrlen;
    if (cab_remap_loopback_https(addr, &storage, &len)) {{
        return real_connect(sockfd, (struct sockaddr *)&storage, len);
    }}

    return real_connect(sockfd, addr, addrlen);
}}
"#,
        default_port = redirect_port,
        host_checks = host_checks
    );

    fs::write(&paths.source, c_code).map_err(|e| CabError::NotFound(e.to_string()))?;

    let status = Command::new("gcc")
        .args([
            "-shared",
            "-fPIC",
            "-o",
            paths
                .library
                .to_str()
                .ok_or_else(|| CabError::NotFound("invalid hook library path".into()))?,
            paths
                .source
                .to_str()
                .ok_or_else(|| CabError::NotFound("invalid hook source path".into()))?,
            "-ldl",
        ])
        .status()
        .map_err(|e| CabError::NotFound(format!("gcc not available: {e}")))?;

    if !status.success() {
        return Err(CabError::NotFound(format!(
            "gcc failed compiling traffic hook (exit {status})"
        )));
    }

    Ok(())
}

/// Grant the running cab-server binary permission to bind 127.0.0.1:443 (Go/agy proxy mode).
pub fn try_setcap_cab_server() -> Result<String, CabError> {
    let exe = std::env::current_exe().map_err(|e| CabError::NotFound(e.to_string()))?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| CabError::NotFound("invalid cab-server path".into()))?;

    if let Ok(output) = Command::new("getcap").arg(exe_str).output() {
        let current = String::from_utf8_lossy(&output.stdout);
        if current.contains("cap_net_bind_service") {
            return Ok(format!(
                "setcap already active on {} — restart cab-server if :443 is not listening",
                exe.display()
            ));
        }
    }

    let apply = Command::new("setcap")
        .args(["cap_net_bind_service=+ep", exe_str])
        .output()
        .map_err(|e| CabError::NotFound(format!("setcap not available: {e}")))?;

    if apply.status.success() {
        return Ok(format!(
            "setcap cap_net_bind_service applied to {} — restart cab-server to listen on 127.0.0.1:443",
            exe.display()
        ));
    }

    let stderr = String::from_utf8_lossy(&apply.stderr);
    Err(CabError::NotFound(format!(
        "setcap failed: {stderr}. Run once: sudo setcap cap_net_bind_service=+ep {}",
        exe.display()
    )))
}

/// Fallback when setcap is unavailable: redirect loopback:443 → CAB HTTPS port via iptables.
pub fn setup_loopback_443_redirect(target_port: u16) -> Result<String, CabError> {
    let check = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-C",
            "OUTPUT",
            "-o",
            "lo",
            "-p",
            "tcp",
            "--dport",
            "443",
            "-j",
            "REDIRECT",
            "--to-ports",
            &target_port.to_string(),
        ])
        .output();

    if let Ok(out) = check {
        if out.status.success() {
            return Ok("iptables redirect already active".into());
        }
    }

    let apply = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-A",
            "OUTPUT",
            "-o",
            "lo",
            "-p",
            "tcp",
            "--dport",
            "443",
            "-j",
            "REDIRECT",
            "--to-ports",
            &target_port.to_string(),
        ])
        .output()
        .map_err(|e| CabError::NotFound(format!("iptables not available: {e}")))?;

    if apply.status.success() {
        return Ok(format!(
            "iptables redirect lo:443 -> lo:{target_port} installed"
        ));
    }

    let stderr = String::from_utf8_lossy(&apply.stderr);
    Err(CabError::NotFound(format!(
        "Could not install iptables redirect (need sudo once): {stderr}. \
         Run: sudo iptables -t nat -A OUTPUT -o lo -p tcp --dport 443 -j REDIRECT --to-ports {target_port}"
    )))
}

pub fn write_wrapper_script(
    script_path: &Path,
    hook_library: &Path,
    command: &str,
    extra_env: &[(&str, &str)],
) -> Result<(), CabError> {
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent).map_err(|e| CabError::NotFound(e.to_string()))?;
    }

    let mut lines = vec![
        "#!/usr/bin/env bash".to_string(),
        "set -euo pipefail".to_string(),
        format!(
            "export LD_PRELOAD=\"{}${{LD_PRELOAD:+:$LD_PRELOAD}}\"",
            hook_library.display()
        ),
    ];
    for (k, v) in extra_env {
        lines.push(format!("export {k}=\"{v}\""));
    }
    lines.push(format!("exec {command} \"$@\""));

    fs::write(&script_path, lines.join("\n") + "\n")
        .map_err(|e| CabError::NotFound(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(script_path)
            .map_err(|e| CabError::NotFound(e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(script_path, perms).map_err(|e| CabError::NotFound(e.to_string()))?;
    }
    Ok(())
}

/// Hostnames used by Antigravity CLI (agy) for model traffic.
pub const ANTIGRAVITY_PROXY_HOSTS: &[&str] = &[
    "daily-cloudcode-pa.googleapis.com",
    "cloudcode-pa.googleapis.com",
];

pub const CLAUDE_DESKTOP_PROXY_HOSTS: &[&str] = &["api.anthropic.com"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_hosts_non_empty() {
        assert_eq!(ANTIGRAVITY_PROXY_HOSTS.len(), 2);
    }
}
