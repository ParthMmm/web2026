export interface AppleMusicKey {
  keyId: string;
  /** The MusicKit `.p8` file's contents, PEM armor included. */
  privateKey: string;
  teamId: string;
}

interface JwtHeader {
  alg: "ES256";
  kid: string;
}

interface JwtClaims {
  exp: number;
  iat: number;
  iss: string;
}

const TOKEN_LIFETIME_SECONDS = 60 * 60 * 12;
const PEM_ARMOR = /-----(?:BEGIN|END) PRIVATE KEY-----|\s/gu;

const base64url = (bytes: Uint8Array): string =>
  btoa(String.fromCodePoint(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "");

const encodeJson = (value: JwtClaims | JwtHeader): string =>
  base64url(new TextEncoder().encode(JSON.stringify(value)));

const importKey = (pem: string): Promise<CryptoKey> => {
  const der = Uint8Array.from(atob(pem.replace(PEM_ARMOR, "")), (char) => {
    const code = char.codePointAt(0);
    if (code === undefined) {
      throw new Error("The MusicKit private key is not valid UTF-8.");
    }
    return code;
  });
  return crypto.subtle.importKey(
    "pkcs8",
    der,
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign"]
  );
};

/** An ES256 developer token for the Apple Music API, valid for twelve hours. */
export const signDeveloperToken = async (
  key: AppleMusicKey,
  nowSeconds: number
): Promise<{ expiresAt: number; token: string }> => {
  const expiresAt = nowSeconds + TOKEN_LIFETIME_SECONDS;
  const signingInput = `${encodeJson({ alg: "ES256", kid: key.keyId })}.${encodeJson(
    { exp: expiresAt, iat: nowSeconds, iss: key.teamId }
  )}`;
  // WebCrypto's ECDSA signature is already the raw r||s form JWS expects.
  const signature = await crypto.subtle.sign(
    { hash: "SHA-256", name: "ECDSA" },
    await importKey(key.privateKey),
    new TextEncoder().encode(signingInput)
  );
  return {
    expiresAt,
    token: `${signingInput}.${base64url(new Uint8Array(signature))}`,
  };
};
