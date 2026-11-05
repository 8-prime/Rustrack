// TypeScript port of `decode_frame` in `shared/src/protocol/protocol.rs`.
//
// Wire layout (little-endian):
//   u16 record_count
//   repeat count:
//     u16 serial_len
//     [u8; serial_len] serial (UTF-8)
//     f32 x, f32 y, f32 theta

export interface AgvRecord {
  serial: string;
  x: number;
  y: number;
  theta: number;
}

const utf8 = new TextDecoder();

/**
 * Decode one binary pose frame. Returns `[]` if the buffer is truncated
 * (mirrors the Rust decoder's `None` on short/invalid input).
 */
export function decodeFrame(buf: ArrayBuffer): AgvRecord[] {
  const view = new DataView(buf);
  const bytes = new Uint8Array(buf);
  let offset = 0;

  if (offset + 2 > view.byteLength) return [];
  const count = view.getUint16(offset, true);
  offset += 2;

  const records: AgvRecord[] = [];
  for (let i = 0; i < count; i++) {
    if (offset + 2 > view.byteLength) return [];
    const serialLen = view.getUint16(offset, true);
    offset += 2;

    if (offset + serialLen > view.byteLength) return [];
    const serial = utf8.decode(bytes.subarray(offset, offset + serialLen));
    offset += serialLen;

    if (offset + 12 > view.byteLength) return [];
    const x = view.getFloat32(offset, true);
    const y = view.getFloat32(offset + 4, true);
    const theta = view.getFloat32(offset + 8, true);
    offset += 12;

    records.push({ serial, x, y, theta });
  }

  return records;
}

/** Text frame: start streaming a robot's route, or stop with a null serial. */
export function selectRouteMessage(serial: string | null): string {
  return JSON.stringify({ serial });
}

/**
 * Text frame: a robot's remaining route (see `backend/src/runtime/route.rs`).
 * Sent when it changes, and once right after the robot is selected.
 */
export interface RouteMessage {
  serial: string;
  /** World metres, in driving order. Empty if the robot reports no geometry. */
  points: [number, number][];
  /** Leading points that reach the next node. */
  currentPoints: number;
  /** Leading points that are released; the rest are the horizon. */
  releasedPoints: number;
}
