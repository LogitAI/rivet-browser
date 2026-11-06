//! The local control channel: one request and one response per connection, after a handshake.
//!
//! Every message is an `Envelope` written as a four byte little-endian length and then its
//! Protocol Buffers bytes. The client sends `Hello`, the server answers `HelloAccepted` or
//! `HelloRejected`, then the client sends one `ControlRequest` and the server one
//! `ControlResponse`. The stream can be a named pipe or the in-memory pair below, so the same code
//! runs in tests and against a browser.

use std::io::{Read, Write};

use prost::Message;

use crate::generated::{
    envelope, ControlRequest, ControlResponse, Envelope, HelloAccepted, HelloRejected,
};

/// The largest message either side accepts. A window capture is the largest one.
pub const MAX_MESSAGE_BYTES: usize = 128 * 1024 * 1024;

/// The pipe a Rivet window serves its control channel on. The browser process and window handle
/// name it, so a client reaches exactly the window of its session.
pub fn pipe_name(process_id: u32, window_handle: isize) -> String {
    format!(r"\\.\pipe\rivet-control-{process_id}-{window_handle:x}")
}

/// A failure of the channel itself, as opposed to a command that failed.
#[derive(Debug)]
pub enum ChannelError {
    Io(std::io::Error),
    Decode(String),
    TooLarge(usize),
    Rejected(String),
    Unexpected(&'static str),
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "the control channel failed: {error}"),
            Self::Decode(error) => {
                write!(f, "the control channel sent an unreadable message: {error}")
            }
            Self::TooLarge(size) => write!(f, "a control message of {size} bytes is too large"),
            Self::Rejected(reason) => {
                write!(f, "the Rivet window refused the connection: {reason}")
            }
            Self::Unexpected(what) => write!(f, "the control channel expected {what}"),
        }
    }
}

impl std::error::Error for ChannelError {}

impl From<std::io::Error> for ChannelError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Writes one envelope with its length in front.
pub fn write_envelope(
    stream: &mut impl Write,
    message: envelope::Message,
) -> Result<(), ChannelError> {
    let bytes = Envelope {
        message: Some(message),
    }
    .encode_to_vec();
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ChannelError::TooLarge(bytes.len()));
    }
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

/// Reads one envelope.
pub fn read_envelope(stream: &mut impl Read) -> Result<envelope::Message, ChannelError> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(ChannelError::TooLarge(length));
    }
    let mut bytes = vec![0u8; length];
    stream.read_exact(&mut bytes)?;
    Envelope::decode(bytes.as_slice())
        .map_err(|error| ChannelError::Decode(error.to_string()))?
        .message
        .ok_or(ChannelError::Unexpected("a message in the envelope"))
}

/// Sends one request after the handshake and returns the answer.
pub fn call(
    stream: &mut (impl Read + Write),
    request: &ControlRequest,
) -> Result<ControlResponse, ChannelError> {
    write_envelope(stream, envelope::Message::Hello(crate::hello()))?;
    match read_envelope(stream)? {
        envelope::Message::HelloAccepted(_) => {}
        envelope::Message::HelloRejected(rejected) => {
            return Err(ChannelError::Rejected(rejected.reason))
        }
        _ => return Err(ChannelError::Unexpected("a handshake answer")),
    }
    write_envelope(stream, envelope::Message::Request(request.clone()))?;
    match read_envelope(stream)? {
        envelope::Message::Response(response) => Ok(response),
        _ => Err(ChannelError::Unexpected("a response")),
    }
}

/// Takes the client's handshake and its request. A peer built from another protocol definition is
/// told why and gets no request read.
pub fn accept(stream: &mut (impl Read + Write)) -> Result<ControlRequest, ChannelError> {
    let envelope::Message::Hello(peer) = read_envelope(stream)? else {
        return Err(ChannelError::Unexpected("a hello"));
    };
    if let Err(error) = crate::validate_hello(&peer) {
        let reason = error.to_string();
        write_envelope(
            stream,
            envelope::Message::HelloRejected(HelloRejected {
                reason: reason.clone(),
            }),
        )?;
        return Err(ChannelError::Rejected(reason));
    }
    write_envelope(stream, envelope::Message::HelloAccepted(HelloAccepted {}))?;
    match read_envelope(stream)? {
        envelope::Message::Request(request) => Ok(request),
        _ => Err(ChannelError::Unexpected("a request")),
    }
}

/// Answers the request taken by [`accept`].
pub fn reply(stream: &mut impl Write, response: ControlResponse) -> Result<(), ChannelError> {
    write_envelope(stream, envelope::Message::Response(response))
}

/// One end of an in-memory duplex stream, for tests of both sides of the channel.
pub struct MemoryStream {
    send: std::sync::mpsc::Sender<Vec<u8>>,
    receive: std::sync::mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    at: usize,
}

/// Two connected in-memory ends. What one writes the other reads; a dropped end reads as closed.
pub fn memory_pair() -> (MemoryStream, MemoryStream) {
    let (a_send, b_receive) = std::sync::mpsc::channel();
    let (b_send, a_receive) = std::sync::mpsc::channel();
    (
        MemoryStream {
            send: a_send,
            receive: a_receive,
            buffer: Vec::new(),
            at: 0,
        },
        MemoryStream {
            send: b_send,
            receive: b_receive,
            buffer: Vec::new(),
            at: 0,
        },
    )
}

impl Read for MemoryStream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.at == self.buffer.len() {
            match self.receive.recv() {
                Ok(bytes) => {
                    self.buffer = bytes;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let count = out.len().min(self.buffer.len() - self.at);
        out[..count].copy_from_slice(&self.buffer[self.at..self.at + count]);
        self.at += count;
        Ok(count)
    }
}

impl Write for MemoryStream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.send
            .send(bytes.to_vec())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::{
        control_request, control_response, Done, Hello, PageList, ProtocolVersion,
    };

    #[test]
    fn fr4_one_request_and_response_cross_the_in_memory_channel() {
        let (mut client, mut server) = memory_pair();
        let serving = std::thread::spawn(move || {
            let request = accept(&mut server).unwrap();
            assert!(matches!(
                request.command,
                Some(control_request::Command::PageList(_))
            ));
            reply(
                &mut server,
                ControlResponse {
                    result: Some(control_response::Result::Done(Done {})),
                    ..Default::default()
                },
            )
            .unwrap();
        });
        let response = call(
            &mut client,
            &ControlRequest {
                command: Some(control_request::Command::PageList(PageList {})),
                ..Default::default()
            },
        )
        .unwrap();
        serving.join().unwrap();
        assert!(matches!(
            response.result,
            Some(control_response::Result::Done(_))
        ));
    }

    #[test]
    fn fr4_a_peer_of_another_definition_is_refused_before_its_request() {
        let (mut client, mut server) = memory_pair();
        let serving = std::thread::spawn(move || accept(&mut server).is_err());
        write_envelope(
            &mut client,
            envelope::Message::Hello(Hello {
                version: Some(ProtocolVersion { major: 1, minor: 0 }),
                schema_hash: "another definition".into(),
            }),
        )
        .unwrap();
        assert!(matches!(
            read_envelope(&mut client).unwrap(),
            envelope::Message::HelloRejected(_)
        ));
        assert!(serving.join().unwrap());
    }

    #[test]
    fn fr4_an_oversized_length_is_refused_without_reading_it() {
        let bytes = ((MAX_MESSAGE_BYTES + 1) as u32).to_le_bytes();
        assert!(matches!(
            read_envelope(&mut bytes.as_slice()),
            Err(ChannelError::TooLarge(_))
        ));
    }
}
