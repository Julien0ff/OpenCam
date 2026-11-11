use crate::commands::{ControlCommand, ControlResponse};
use crate::ProtocolError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_CONTROL_FRAME_SIZE: usize = 64 * 1024; // 64 KB max for JSON/Bincode RPC frame

/// Reads a length-prefixed `ControlCommand` from an asynchronous stream.
pub async fn read_command<R: AsyncRead + Unpin>(reader: &mut R) -> Result<ControlCommand, ProtocolError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > MAX_CONTROL_FRAME_SIZE {
        return Err(ProtocolError::BufferTooShort {
            needed: len,
            actual: MAX_CONTROL_FRAME_SIZE,
        });
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let cmd = bincode::deserialize(&buf)?;
    Ok(cmd)
}

/// Writes a length-prefixed `ControlCommand` to an asynchronous stream.
pub async fn write_command<W: AsyncWrite + Unpin>(
    writer: &mut W,
    command: &ControlCommand,
) -> Result<(), ProtocolError> {
    let payload = bincode::serialize(command)?;
    let len = payload.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads a length-prefixed `ControlResponse` from an asynchronous stream.
pub async fn read_response<R: AsyncRead + Unpin>(reader: &mut R) -> Result<ControlResponse, ProtocolError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > MAX_CONTROL_FRAME_SIZE {
        return Err(ProtocolError::BufferTooShort {
            needed: len,
            actual: MAX_CONTROL_FRAME_SIZE,
        });
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let resp = bincode::deserialize(&buf)?;
    Ok(resp)
}

/// Writes a length-prefixed `ControlResponse` to an asynchronous stream.
pub async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &ControlResponse,
) -> Result<(), ProtocolError> {
    let payload = bincode::serialize(response)?;
    let len = payload.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CameraCapabilities, DeviceInfo};

    #[tokio::test]
    async fn test_async_command_and_response_roundtrip() {
        let (mut client_io, mut server_io) = tokio::io::duplex(1024);

        let command = ControlCommand::SetZoom { factor: 3.5 };
        write_command(&mut client_io, &command).await.unwrap();

        let received_cmd = read_command(&mut server_io).await.unwrap();
        assert_eq!(command, received_cmd);

        let response = ControlResponse::Handshake {
            device_info: DeviceInfo {
                device_id: "test-device".to_string(),
                device_name: "Pixel Test".to_string(),
                os: "Android".to_string(),
                protocol_version: 1,
                battery_level: Some(0.95),
                is_charging: Some(false),
            },
            capabilities: CameraCapabilities {
                cameras: vec![],
                supported_codecs: vec![],
                supported_pixel_formats: vec![],
            },
        };

        write_response(&mut server_io, &response).await.unwrap();
        let received_resp = read_response(&mut client_io).await.unwrap();
        assert_eq!(response, received_resp);
    }
}
