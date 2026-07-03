use bytes::{BufMut, Bytes, BytesMut};

pub const RECORD_SIZE: usize = size_of::<AgvRecord>();

pub struct AgvRecord {
    pub agv_id: u32,
    pub x: f32,
    pub y: f32,
    pub theta: f32,
}

pub fn encode_frame(states: &[AgvRecord]) -> Bytes {
    let mut buf = BytesMut::with_capacity(size_of::<u16>() + states.len() * RECORD_SIZE);

    buf.put_u16_le(states.len() as u16);
    for s in states {
        buf.put_u32_le(s.agv_id);
        buf.put_f32_le(s.x);
        buf.put_f32_le(s.y);
        buf.put_f32_le(s.theta);
    }

    buf.freeze()
}

pub fn decode_frame(data: &[u8]) -> Option<Vec<AgvRecord>> {
    if data.len() < 2 {
        // Payload must at least contain item count
        return None;
    }
    let count = u16::from_le_bytes([data[0], data[1]]) as usize;
    if data.len() < size_of::<u16>() + count * RECORD_SIZE {
        return None;
    }
    let mut states = Vec::with_capacity(count);
    let mut offset = 2;
    for _ in 0..count {
        let agv_id = u32::from_le_bytes(data[offset..offset + size_of::<u32>()].try_into().ok()?);
        offset += size_of::<u32>();
        let x = f32::from_le_bytes(data[offset..offset + size_of::<f32>()].try_into().ok()?);
        offset += size_of::<f32>();
        let y = f32::from_le_bytes(data[offset..offset + size_of::<f32>()].try_into().ok()?);
        offset += size_of::<f32>();
        let theta = f32::from_le_bytes(data[offset..offset + size_of::<f32>()].try_into().ok()?);
        offset += size_of::<f32>();
        states.push(AgvRecord {
            agv_id,
            x,
            y,
            theta,
        });
    }
    Some(states)
}
