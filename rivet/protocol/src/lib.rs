//! The generated local-control protocol shared by the CLI and Windows runtime.
//!
//! `control.proto` is the single message definition. `build.rs` generates the Rust codec during
//! every build and embeds a SHA-256 hash of that definition into the protocol identity.

use prost::Message;

/// The generated Protocol Buffers messages.
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/rivet.control.v1.rs"));
}

pub use generated::{Hello, HelloAccepted, HelloRejected, ProtocolVersion};

/// The protocol major version. A different major version is never accepted.
pub const PROTOCOL_MAJOR: u32 = 1;
/// The protocol minor version. A different minor version is rejected until compatibility rules exist.
pub const PROTOCOL_MINOR: u32 = 0;
/// SHA-256 of `control.proto` at build time.
pub const CONTROL_SCHEMA_HASH: &str = env!("RIVET_CONTROL_SCHEMA_HASH");

/// A handshake error that can safely be returned to the peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    MissingVersion,
    VersionMismatch {
        received_major: u32,
        received_minor: u32,
    },
    SchemaMismatch {
        received: String,
    },
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVersion => write!(f, "the peer did not send a protocol version"),
            Self::VersionMismatch {
                received_major,
                received_minor,
            } => write!(
                f,
                "protocol version {received_major}.{received_minor} does not match {}.{}",
                PROTOCOL_MAJOR, PROTOCOL_MINOR
            ),
            Self::SchemaMismatch { received } => write!(
                f,
                "protocol schema hash {received} does not match {CONTROL_SCHEMA_HASH}"
            ),
        }
    }
}

impl std::error::Error for HandshakeError {}

/// Returns Rivet's own protocol identity.
pub fn hello() -> Hello {
    Hello {
        version: Some(ProtocolVersion {
            major: PROTOCOL_MAJOR,
            minor: PROTOCOL_MINOR,
        }),
        schema_hash: CONTROL_SCHEMA_HASH.to_owned(),
    }
}

/// Verifies that a peer was built from this protocol definition.
pub fn validate_hello(peer: &Hello) -> Result<(), HandshakeError> {
    let version = peer
        .version
        .as_ref()
        .ok_or(HandshakeError::MissingVersion)?;
    if version.major != PROTOCOL_MAJOR || version.minor != PROTOCOL_MINOR {
        return Err(HandshakeError::VersionMismatch {
            received_major: version.major,
            received_minor: version.minor,
        });
    }
    if peer.schema_hash != CONTROL_SCHEMA_HASH {
        return Err(HandshakeError::SchemaMismatch {
            received: peer.schema_hash.clone(),
        });
    }
    Ok(())
}

/// Encodes a message with the generated Protocol Buffers codec.
pub fn encode(message: &impl Message) -> Vec<u8> {
    message.encode_to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(value: &str) -> Vec<u8> {
        let compact = value
            .lines()
            .filter(|line| !line.starts_with('#'))
            .flat_map(str::chars)
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        compact
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn pr1_generated_hello_codec_matches_the_checked_in_vector() {
        let bytes = decode_hex(include_str!("../vectors/hello-v1.hex"));
        let hello = Hello::decode(bytes.as_slice()).unwrap();
        assert_eq!(hello.version, Some(ProtocolVersion { major: 1, minor: 0 }));
        assert_eq!(hello.schema_hash, "schema");
        assert_eq!(encode(&hello), bytes);
    }

    #[test]
    fn pr1_mismatched_schema_hash_is_rejected() {
        let mut peer = hello();
        peer.schema_hash = "a different definition".into();
        assert_eq!(
            validate_hello(&peer),
            Err(HandshakeError::SchemaMismatch {
                received: "a different definition".into()
            })
        );
    }

    #[test]
    fn pr1_mismatched_protocol_version_is_rejected() {
        let peer = Hello {
            version: Some(ProtocolVersion { major: 1, minor: 1 }),
            schema_hash: CONTROL_SCHEMA_HASH.into(),
        };
        assert_eq!(
            validate_hello(&peer),
            Err(HandshakeError::VersionMismatch {
                received_major: 1,
                received_minor: 1
            })
        );
    }
}
