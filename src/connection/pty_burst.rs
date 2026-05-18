//! Coalesce PTY reads on Unix so one shell history redraw triggers one UI update.

use std::io::{Read, Result};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::io::RawFd;

#[cfg(unix)]
fn poll_readable(fd: RawFd) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pfd, 1, 0) > 0 }
}

/// After a blocking `read`, pull every byte already waiting on the PTY fd (timeout 0 poll).
#[cfg(unix)]
pub fn append_available(fd: RawFd, reader: &mut dyn Read, buf: &mut [u8], out: &mut Vec<u8>) -> Result<()> {
    loop {
        if !poll_readable(fd) {
            break;
        }
        match reader.read(buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Keep reading until the PTY has been quiet for a short interval (caps wait at 3ms).
#[cfg(unix)]
pub fn append_until_idle(
    fd: RawFd,
    reader: &mut dyn Read,
    buf: &mut [u8],
    out: &mut Vec<u8>,
) -> Result<()> {
    append_available(fd, reader, buf, out)?;

    const QUIET: Duration = Duration::from_micros(500);
    const MAX_WAIT: Duration = Duration::from_millis(3);
    let deadline = Instant::now() + MAX_WAIT;
    let mut last_data = Instant::now();

    while Instant::now() < deadline {
        if poll_readable(fd) {
            match reader.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    last_data = Instant::now();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
        } else if last_data.elapsed() >= QUIET {
            break;
        } else {
            std::thread::sleep(Duration::from_micros(200));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn append_available(
    _fd: (),
    _reader: &mut dyn Read,
    _buf: &mut [u8],
    _out: &mut Vec<u8>,
) -> Result<()> {
    Ok(())
}

#[cfg(not(unix))]
pub fn append_until_idle(
    _fd: (),
    _reader: &mut dyn Read,
    _buf: &mut [u8],
    _out: &mut Vec<u8>,
) -> Result<()> {
    Ok(())
}
