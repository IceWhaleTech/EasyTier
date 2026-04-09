const MASK_64 = (1n << 64n) - 1n;
const textEncoder = new TextEncoder();

type SipState = {
  v0: bigint;
  v1: bigint;
  v2: bigint;
  v3: bigint;
};

class SipHasher13 {
  private readonly state: SipState = {
    v0: 0x736f6d6570736575n,
    v1: 0x646f72616e646f6dn,
    v2: 0x6c7967656e657261n,
    v3: 0x7465646279746573n,
  };

  private length = 0;
  private tail = 0n;
  private ntail = 0;

  write(bytes: Uint8Array): void {
    const inputLength = bytes.length;
    this.length += inputLength;

    let needed = 0;

    if (this.ntail !== 0) {
      needed = 8 - this.ntail;
      this.tail |=
        u8To64Le(bytes, 0, Math.min(inputLength, needed)) <<
        BigInt(8 * this.ntail);

      if (inputLength < needed) {
        this.ntail += inputLength;
        return;
      }

      this.state.v3 ^= this.tail;
      this.cRounds();
      this.state.v0 ^= this.tail;
      this.ntail = 0;
    }

    const len = inputLength - needed;
    const left = len & 0x7;

    let index = needed;
    while (index < len - left) {
      const word = loadU64Le(bytes, index);
      this.state.v3 ^= word;
      this.cRounds();
      this.state.v0 ^= word;
      index += 8;
    }

    this.tail = u8To64Le(bytes, index, left);
    this.ntail = left;
  }

  finish(): bigint {
    const state = { ...this.state };
    const b = ((BigInt(this.length) & 0xffn) << 56n) | this.tail;

    state.v3 ^= b;
    compress(state);
    state.v0 ^= b;

    state.v2 ^= 0xffn;
    compress(state);
    compress(state);
    compress(state);

    return (state.v0 ^ state.v1 ^ state.v2 ^ state.v3) & MASK_64;
  }

  private cRounds(): void {
    compress(this.state);
  }
}

export function generateNetworkSecretDigestHex(
  networkName: string,
  networkSecret: string,
): string {
  const hasher = new SipHasher13();
  hasher.write(textEncoder.encode(networkName));
  hasher.write(textEncoder.encode(networkSecret));

  const digest = new Uint8Array(32);
  for (let index = 0; index < digest.length; index += 8) {
    const shard = bigIntToBeBytes(hasher.finish());
    digest.set(shard, index);
    hasher.write(digest.subarray(0, index + 8));
  }

  return [...digest]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function compress(state: SipState): void {
  state.v0 = (state.v0 + state.v1) & MASK_64;
  state.v2 = (state.v2 + state.v3) & MASK_64;
  state.v1 = rotateLeft64(state.v1, 13);
  state.v1 ^= state.v0;
  state.v3 = rotateLeft64(state.v3, 16);
  state.v3 ^= state.v2;
  state.v0 = rotateLeft64(state.v0, 32);

  state.v2 = (state.v2 + state.v1) & MASK_64;
  state.v0 = (state.v0 + state.v3) & MASK_64;
  state.v1 = rotateLeft64(state.v1, 17);
  state.v1 ^= state.v2;
  state.v3 = rotateLeft64(state.v3, 21);
  state.v3 ^= state.v0;
  state.v2 = rotateLeft64(state.v2, 32);
}

function rotateLeft64(value: bigint, shift: number): bigint {
  const amount = BigInt(shift);
  return ((value << amount) & MASK_64) | (value >> (64n - amount));
}

function loadU64Le(bytes: Uint8Array, offset: number): bigint {
  let value = 0n;

  for (let index = 0; index < 8; index += 1) {
    value |= BigInt(bytes[offset + index] ?? 0) << BigInt(index * 8);
  }

  return value;
}

function u8To64Le(bytes: Uint8Array, offset: number, length: number): bigint {
  let value = 0n;

  for (let index = 0; index < length; index += 1) {
    value |= BigInt(bytes[offset + index] ?? 0) << BigInt(index * 8);
  }

  return value;
}

function bigIntToBeBytes(value: bigint): Uint8Array {
  const output = new Uint8Array(8);

  for (let index = 0; index < output.length; index += 1) {
    const shift = BigInt((output.length - 1 - index) * 8);
    output[index] = Number((value >> shift) & 0xffn);
  }

  return output;
}
