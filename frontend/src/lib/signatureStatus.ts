import type { SignatureStatus } from "../ipc/RepoClient";

export type SignatureTone = "success" | "warning" | "danger" | "neutral";

const LABELS: Record<Exclude<SignatureStatus, "unsigned">, { tone: SignatureTone; text: string }> = {
  good: { tone: "success", text: "Good signature" },
  goodUntrusted: { tone: "warning", text: "Valid signature, signer not trusted" },
  unknownKey: { tone: "warning", text: "Signed with an unknown key" },
  expired: { tone: "warning", text: "Signature expired" },
  expiredKey: { tone: "warning", text: "Signed with an expired key" },
  bad: { tone: "danger", text: "Bad signature" },
  revokedKey: { tone: "danger", text: "Signed with a revoked key" },
  error: { tone: "neutral", text: "Signature could not be checked" },
};

export function signatureLabel(status: Exclude<SignatureStatus, "unsigned">) {
  return LABELS[status];
}

/** gpg fingerprints: last 16 hex chars (the long key id). ssh: `SHA256:` + first 12 chars. */
export function shortKey(key: string): string {
  if (key.startsWith("SHA256:")) return key.slice(0, "SHA256:".length + 12);
  return key.length > 16 ? key.slice(-16) : key;
}
