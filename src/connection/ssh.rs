use std::collections::HashMap;
use std::future::Future;
use std::sync::{mpsc, Arc};

use russh::client::{self, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh::{ChannelMsg, Disconnect};
use tokio::sync::mpsc::unbounded_channel;

use crate::connection::{
    emit_conn_data, ssh_keys, ConnIn, ConnOut, ConnectionHandle, ConnectionState, RepaintNotifier,
};
use crate::storage::types::SavedConnection;

struct SshClient;

impl client::Handler for SshClient {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }
}

pub fn connect_ssh(
    config: &SavedConnection,
    env_vars: &HashMap<String, String>,
    rows: u16,
    cols: u16,
) -> Result<ConnectionHandle, String> {
    let host = config
        .ssh_host
        .clone()
        .ok_or_else(|| "SSH host not configured".to_string())?;
    let port = config.ssh_port.unwrap_or(22);
    let user = config
        .ssh_user
        .clone()
        .ok_or_else(|| "SSH user not configured".to_string())?;
    let saved_password = config.ssh_password.clone();

    let (to_conn_tx, to_conn_rx) = mpsc::channel::<ConnOut>();
    let (from_conn_tx, from_conn_rx) = mpsc::channel::<ConnIn>();

    let host_clone = host.clone();
    let from_tx = from_conn_tx.clone();
    let env_vars = env_vars.clone();
    let repaint = RepaintNotifier::default();
    let ssh_repaint = repaint.clone();

    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        if let Err(msg) = rt.block_on(run_ssh(
            &host_clone,
            port,
            &user,
            saved_password,
            rows,
            cols,
            &env_vars,
            to_conn_rx,
            from_tx,
            ssh_repaint,
        )) {
            let _ = from_conn_tx.send(ConnIn::StateChanged(ConnectionState::Error(msg)));
        }
    });

    Ok(ConnectionHandle::new(
        to_conn_tx,
        from_conn_rx,
        thread,
        std::thread::spawn(|| {}),
        repaint,
    ))
}

async fn run_ssh(
    host: &str,
    port: u16,
    user: &str,
    saved_password: Option<String>,
    rows: u16,
    cols: u16,
    env_vars: &HashMap<String, String>,
    to_conn_rx: mpsc::Receiver<ConnOut>,
    from_tx: mpsc::Sender<ConnIn>,
    repaint: RepaintNotifier,
) -> Result<(), String> {
    let (out_async_tx, mut out_async_rx) = unbounded_channel::<ConnOut>();
    std::thread::spawn(move || {
        while let Ok(msg) = to_conn_rx.recv() {
            if out_async_tx.send(msg).is_err() {
                break;
            }
        }
    });

    let ssh_config = Arc::new(client::Config::default());
    let mut handle = client::connect(ssh_config, (host, port), SshClient)
        .await
        .map_err(|e| e.to_string())?;

    let password = saved_password
        .or_else(|| std::env::var("SSH_PASSWORD").ok())
        .or_else(|| std::env::var("RSTERM_SSH_PASSWORD").ok());

    authenticate(&mut handle, user, password.as_deref()).await?;

    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;

    let cols_u = cols.max(1) as u32;
    let rows_u = rows.max(1) as u32;

    channel
        .request_pty(false, "xterm-256color", cols_u, rows_u, 0, 0, &[])
        .await
        .map_err(|e| e.to_string())?;

    for (key, value) in env_vars {
        let _ = channel.set_env(true, key, value).await;
    }

    channel
        .request_shell(true)
        .await
        .map_err(|e| e.to_string())?;

    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Connected));

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        emit_conn_data(&from_tx, &repaint, data.to_vec());
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        emit_conn_data(&from_tx, &repaint, data.to_vec());
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        break;
                    }
                    _ => {}
                }
            }
            out = out_async_rx.recv() => {
                match out {
                    Some(ConnOut::Data(data)) => {
                        if channel.data(&data[..]).await.is_err() {
                            break;
                        }
                    }
                    Some(ConnOut::Resize(rows, cols)) => {
                        let _ = channel
                            .window_change(cols.max(1) as u32, rows.max(1) as u32, 0, 0)
                            .await;
                    }
                    Some(ConnOut::Winch) => {}
                    Some(ConnOut::Close) | None => break,
                }
            }
        }
    }

    let _ = channel.close().await;
    let _ = handle
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;
    let _ = from_tx.send(ConnIn::StateChanged(ConnectionState::Disconnected));
    Ok(())
}

async fn authenticate(
    handle: &mut Handle<SshClient>,
    user: &str,
    password: Option<&str>,
) -> Result<(), String> {
    for path in ssh_keys::default_key_paths() {
        if !path.is_file() {
            continue;
        }
        let Ok(key) = load_secret_key(&path, None) else {
            continue;
        };
        let hash = handle
            .best_supported_rsa_hash()
            .await
            .map_err(|e| e.to_string())?
            .flatten();
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
        if handle
            .authenticate_publickey(user, key)
            .await
            .map(|r| r.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
    }

    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        if handle
            .authenticate_password(user, pw)
            .await
            .map(|r| r.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        if try_keyboard_interactive(handle, user, pw).await {
            return Ok(());
        }
    }

    if handle
        .authenticate_none(user)
        .await
        .map(|r| r.success())
        .unwrap_or(false)
    {
        return Ok(());
    }

    Err("SSH authentication failed (tried public keys, password, and keyboard-interactive)".into())
}

async fn try_keyboard_interactive(
    handle: &mut Handle<SshClient>,
    user: &str,
    password: &str,
) -> bool {
    let mut resp = match handle
        .authenticate_keyboard_interactive_start(user, None::<String>)
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    loop {
        match resp {
            KeyboardInteractiveAuthResponse::Success => return true,
            KeyboardInteractiveAuthResponse::Failure { .. } => return false,
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers: Vec<String> = prompts
                    .iter()
                    .map(|_| password.to_string())
                    .collect();
                resp = match handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                {
                    Ok(r) => r,
                    Err(_) => return false,
                };
            }
        }
    }
}
