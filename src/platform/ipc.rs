use std::io;

use crate::ipc::{LocalStream, LocalStreamRead};

#[cfg(unix)]
mod imp {
    use std::io::{self, Read};

    use interprocess::local_socket::traits::Stream as _;

    use crate::ipc::{is_connection_closed_error, LocalStream, LocalStreamRead};

    pub(super) fn set_local_stream_polling(
        stream: &mut LocalStream,
        enabled: bool,
    ) -> io::Result<()> {
        stream.set_nonblocking(enabled)
    }

    pub(super) fn poll_local_stream_read(
        stream: &mut LocalStream,
        buf: &mut [u8],
    ) -> io::Result<LocalStreamRead> {
        match stream.read(buf) {
            Ok(0) => Ok(LocalStreamRead::Closed),
            Ok(_) => Ok(LocalStreamRead::Data),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(LocalStreamRead::Pending),
            Err(err) => Err(err),
        }
    }

    pub(super) fn probe_stream_closed(stream: &mut LocalStream) -> io::Result<bool> {
        stream.set_nonblocking(true)?;
        let mut probe = [0u8; 1];
        let status = match stream.read(&mut probe) {
            Ok(0) => Ok(true),
            Ok(_) => Ok(true),
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(false)
            }
            Err(err) if is_connection_closed_error(&err) => Ok(true),
            Err(err) => Err(err),
        };
        stream.set_nonblocking(false)?;
        status
    }
}

#[cfg(windows)]
mod imp {
    use std::io::{self, Read};
    use std::os::windows::io::{AsHandle, AsRawHandle};

    use crate::ipc::{is_connection_closed_error, LocalStream, LocalStreamRead};

    pub(super) fn set_local_stream_polling(
        _stream: &mut LocalStream,
        _enabled: bool,
    ) -> io::Result<()> {
        Ok(())
    }

    pub(super) fn poll_local_stream_read(
        stream: &mut LocalStream,
        buf: &mut [u8],
    ) -> io::Result<LocalStreamRead> {
        match named_pipe_available(stream)? {
            None => Ok(LocalStreamRead::Closed),
            Some(0) => Ok(LocalStreamRead::Pending),
            Some(_) => match stream.read(buf) {
                Ok(0) => Ok(LocalStreamRead::Closed),
                Ok(_) => Ok(LocalStreamRead::Data),
                Err(err) if is_connection_closed_error(&err) => Ok(LocalStreamRead::Closed),
                Err(err) => Err(err),
            },
        }
    }

    pub(super) fn probe_stream_closed(stream: &mut LocalStream) -> io::Result<bool> {
        Ok(named_pipe_available(stream)?.is_none())
    }

    fn named_pipe_available(stream: &mut LocalStream) -> io::Result<Option<u32>> {
        let LocalStream::NamedPipe(pipe) = stream;
        let mut available = 0;
        let ok = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                pipe.as_handle().as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            return Ok(Some(available));
        }

        let err = io::Error::last_os_error();
        if is_connection_closed_error(&err) || named_pipe_closed_error(&err) {
            return Ok(None);
        }
        Err(err)
    }

    fn named_pipe_closed_error(err: &io::Error) -> bool {
        matches!(err.raw_os_error(), Some(6 | 109 | 232 | 233))
    }
}

pub(crate) fn set_local_stream_polling(
    stream: &mut LocalStream,
    enabled: bool,
) -> io::Result<()> {
    imp::set_local_stream_polling(stream, enabled)
}

pub(crate) fn poll_local_stream_read(
    stream: &mut LocalStream,
    buf: &mut [u8],
) -> io::Result<LocalStreamRead> {
    imp::poll_local_stream_read(stream, buf)
}

pub(crate) fn probe_stream_closed(stream: &mut LocalStream) -> io::Result<bool> {
    imp::probe_stream_closed(stream)
}
