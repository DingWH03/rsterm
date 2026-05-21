use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::connection::{
    emit_conn_data, ConnIn, ConnOut, ConnectionHandle, ConnectionState, RepaintNotifier,
};
use crate::storage::types::SavedConnection;

pub fn connect_serial(config: &SavedConnection) -> Result<ConnectionHandle, String> {
    let port_name = config
        .serial_port
        .clone()
        .ok_or_else(|| "Serial port not configured".to_string())?;
    let baud = config.serial_baud.unwrap_or(115200);

    let (to_conn_tx, to_conn_rx) = mpsc::channel::<ConnOut>();
    let (from_conn_tx, from_conn_rx) = mpsc::channel::<ConnIn>();

    let port = serialport::new(&port_name, baud)
        .timeout(Duration::from_millis(10))
        .open()
        .map_err(|e| format!("Cannot open serial port {port_name}: {e}"))?;

    let (mut reader, mut writer) = (
        port.try_clone().map_err(|e| e.to_string())?,
        port.try_clone().map_err(|e| e.to_string())?,
    );

    let _ = from_conn_tx.send(ConnIn::StateChanged(ConnectionState::Connected));

    let alive = Arc::new(AtomicBool::new(true));
    let reader_alive = alive.clone();
    let writer_alive = alive.clone();

    let repaint = RepaintNotifier::default();
    let reader_from_tx = from_conn_tx.clone();
    let reader_repaint = repaint.clone();
    let reader_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 32 * 1024];
        while reader_alive.load(Ordering::Relaxed) {
            match reader.read(&mut buf) {
                Ok(0) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(n) => {
                    emit_conn_data(&reader_from_tx, &reader_repaint, buf[..n].to_vec());
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => {
                    reader_alive.store(false, Ordering::Relaxed);
                    let _ = reader_from_tx
                        .send(ConnIn::StateChanged(ConnectionState::Error(e.to_string())));
                    return;
                }
            }
        }
    });

    let writer_thread = std::thread::spawn(move || {
        loop {
            match to_conn_rx.recv() {
                Ok(ConnOut::Data(data)) | Ok(ConnOut::PortData { port: 0, data }) => {
                    let _ = writer.write_all(&data);
                    let _ = writer.flush();
                }
                Ok(ConnOut::PortData { .. }) => {}
                Ok(ConnOut::Resize(_, _)) | Ok(ConnOut::Winch) => {}
                Ok(ConnOut::Close) => {
                    writer_alive.store(false, Ordering::Relaxed);
                    let _ = from_conn_tx.send(ConnIn::StateChanged(ConnectionState::Closed));
                    return;
                }
                Err(_) => {
                    writer_alive.store(false, Ordering::Relaxed);
                    return;
                }
            }
        }
    });

    Ok(ConnectionHandle::new(
        to_conn_tx,
        from_conn_rx,
        reader_thread,
        writer_thread,
        repaint,
    ))
}
