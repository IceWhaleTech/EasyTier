const HANDSHAKE_PACKET_TYPE = 2;
const NOISE_HANDSHAKE_MSG1_PACKET_TYPE = 13;
const NOISE_XX_EPHEMERAL_KEY_SIZE = 32;
const PEER_MANAGER_HEADER_SIZE = 16;

const textDecoder = new TextDecoder();

export type Frame =
  | string
  | ArrayBuffer
  | ArrayBufferView
  | Blob;

export type ExtractedNetworkRouteIdentity = {
  networkName: string;
  routeKey: string;
  routeMode: "digest" | "network-name-only";
  secretDigestHex: string | null;
};

export async function extractNetworkNameFromFrame(
  data: Frame,
): Promise<string> {
  return (await extractNetworkRouteIdentityFromFrame(data)).networkName;
}

export async function extractNetworkRouteIdentityFromFrame(
  data: Frame,
): Promise<ExtractedNetworkRouteIdentity> {
  const frame = await toBytes(data);
  if (frame.byteLength <= PEER_MANAGER_HEADER_SIZE) {
    throw new Error(
      "websocket frame is too short to contain an EasyTier packet",
    );
  }

  const packetType = frame[8];
  const payload = frame.subarray(PEER_MANAGER_HEADER_SIZE);

  switch (packetType) {
    case HANDSHAKE_PACKET_TYPE: {
      const networkName = readProtoStringField(payload, 5);
      const secretDigest = readProtoBytesField(payload, 6);
      const secretDigestHex = toHex(secretDigest);
      return {
        networkName,
        routeKey: `digest:${networkName}:${secretDigestHex}`,
        routeMode: "digest",
        secretDigestHex,
      };
    }
    case NOISE_HANDSHAKE_MSG1_PACKET_TYPE:
      if (payload.byteLength <= NOISE_XX_EPHEMERAL_KEY_SIZE) {
        throw new Error("Noise handshake frame is too short");
      }
      return {
        networkName: readProtoStringField(
          payload.subarray(NOISE_XX_EPHEMERAL_KEY_SIZE),
          2,
        ),
        routeKey: `network:${readProtoStringField(
          payload.subarray(NOISE_XX_EPHEMERAL_KEY_SIZE),
          2,
        )}`,
        routeMode: "network-name-only",
        secretDigestHex: null,
      };
    default:
      throw new Error(`unsupported EasyTier packet type: ${packetType}`);
  }
}

async function toBytes(data: Frame): Promise<Uint8Array> {
  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }

  if (ArrayBuffer.isView(data)) {
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  }

  if (data instanceof Blob) {
    return new Uint8Array(await data.arrayBuffer());
  }

  throw new Error("expected a binary websocket frame");
}

function readProtoStringField(buffer: Uint8Array, fieldNumber: number): string {
  return textDecoder.decode(readProtoBytesField(buffer, fieldNumber));
}

function readProtoBytesField(buffer: Uint8Array, fieldNumber: number): Uint8Array {
  let offset = 0;

  while (offset < buffer.byteLength) {
    const key = readVarint(buffer, offset);
    offset = key.nextOffset;

    const currentFieldNumber = key.value >>> 3;
    const wireType = key.value & 0x07;

    if (currentFieldNumber === fieldNumber) {
      if (wireType !== 2) {
        throw new Error(`field ${fieldNumber} is not length-delimited`);
      }

      const length = readVarint(buffer, offset);
      offset = length.nextOffset;
      const end = offset + length.value;
      if (end > buffer.byteLength) {
        throw new Error("protobuf field extends past frame boundary");
      }

      return buffer.subarray(offset, end);
    }

    offset = skipWireValue(buffer, offset, wireType);
  }

  throw new Error(`protobuf field ${fieldNumber} not found`);
}

function toHex(buffer: Uint8Array): string {
  return [...buffer]
    .map((item) => item.toString(16).padStart(2, "0"))
    .join("");
}

function readVarint(
  buffer: Uint8Array,
  offset: number,
): { value: number; nextOffset: number } {
  let value = 0;
  let shift = 0;
  let cursor = offset;

  while (cursor < buffer.byteLength) {
    const byte = buffer[cursor];
    value |= (byte & 0x7f) << shift;
    cursor += 1;

    if ((byte & 0x80) === 0) {
      return { value, nextOffset: cursor };
    }

    shift += 7;
    if (shift > 35) {
      throw new Error("protobuf varint is too large");
    }
  }

  throw new Error("unexpected end of protobuf varint");
}

function skipWireValue(
  buffer: Uint8Array,
  offset: number,
  wireType: number,
): number {
  switch (wireType) {
    case 0:
      return readVarint(buffer, offset).nextOffset;
    case 1:
      return offset + 8;
    case 2: {
      const length = readVarint(buffer, offset);
      return length.nextOffset + length.value;
    }
    case 5:
      return offset + 4;
    default:
      throw new Error(`unsupported protobuf wire type: ${wireType}`);
  }
}
