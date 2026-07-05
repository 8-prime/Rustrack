use bytes::{BufMut, Bytes, BytesMut};

/// Fixed-size portion of one record: the three `f32` pose components. The serial
/// is variable-length and length-prefixed, so it isn't counted here.
const RECORD_POSE_SIZE: usize = 3 * size_of::<f32>();

/// A single AGV's pose within one frame.
///
/// The AGV is identified by its VDA5050 `serial`, which is the same identity the
/// backend keys its state by, so the renderer can track a robot across frames.
pub struct AgvRecord {
    pub serial: String,
    pub x: f32,
    pub y: f32,
    pub theta: f32,
}

/// Serialize a frame of AGV poses. All integers/floats are little-endian.
///
/// Layout:
/// ```text
///   u16                         record count
///   repeated `count` times:
///     u16                       serial length in bytes
///     [u8; serial length]       serial, UTF-8
///     f32 x, f32 y, f32 theta   pose
/// ```
pub fn encode_frame(states: &[AgvRecord]) -> Bytes {
    let serial_bytes: usize = states.iter().map(|s| s.serial.len()).sum();
    let capacity = size_of::<u16>()
        + states.len() * (size_of::<u16>() + RECORD_POSE_SIZE)
        + serial_bytes;
    let mut buf = BytesMut::with_capacity(capacity);

    buf.put_u16_le(states.len() as u16);
    for s in states {
        buf.put_u16_le(s.serial.len() as u16);
        buf.put_slice(s.serial.as_bytes());
        buf.put_f32_le(s.x);
        buf.put_f32_le(s.y);
        buf.put_f32_le(s.theta);
    }

    buf.freeze()
}

/// Deserialize a frame produced by [`encode_frame`]. Returns `None` if the buffer
/// is truncated or a serial is not valid UTF-8.
pub fn decode_frame(data: &[u8]) -> Option<Vec<AgvRecord>> {
    // Read the record count.
    let count = u16::from_le_bytes(data.get(0..size_of::<u16>())?.try_into().ok()?) as usize;
    let mut offset = size_of::<u16>();

    let mut states = Vec::with_capacity(count);
    for _ in 0..count {
        // Length-prefixed serial.
        let serial_len =
            u16::from_le_bytes(data.get(offset..offset + size_of::<u16>())?.try_into().ok()?)
                as usize;
        offset += size_of::<u16>();

        let serial = String::from_utf8(data.get(offset..offset + serial_len)?.to_vec()).ok()?;
        offset += serial_len;

        // Pose.
        let x = f32::from_le_bytes(data.get(offset..offset + size_of::<f32>())?.try_into().ok()?);
        offset += size_of::<f32>();
        let y = f32::from_le_bytes(data.get(offset..offset + size_of::<f32>())?.try_into().ok()?);
        offset += size_of::<f32>();
        let theta =
            f32::from_le_bytes(data.get(offset..offset + size_of::<f32>())?.try_into().ok()?);
        offset += size_of::<f32>();

        states.push(AgvRecord {
            serial,
            x,
            y,
            theta,
        });
    }

    Some(states)
}
