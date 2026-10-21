import { describe, expect, it } from "vitest";
import { shortKey, signatureLabel } from "./signatureStatus";

describe("signatureLabel", () => {
  it.each([
    ["good", "success", "Good signature"],
    ["goodUntrusted", "warning", "Valid signature, signer not trusted"],
    ["unknownKey", "warning", "Signed with an unknown key"],
    ["expired", "warning", "Signature expired"],
    ["expiredKey", "warning", "Signed with an expired key"],
    ["bad", "danger", "Bad signature"],
    ["revokedKey", "danger", "Signed with a revoked key"],
    ["error", "neutral", "Signature could not be checked"],
  ] as const)("%s", (status, tone, text) => {
    expect(signatureLabel(status)).toEqual({ tone, text });
  });
});

describe("shortKey", () => {
  it("keeps the last 16 hex chars of a gpg fingerprint", () => {
    expect(shortKey("0123456789ABCDEF0123456789ABCDEF01234567")).toBe("89ABCDEF01234567");
  });
  it("keeps SHA256: plus 12 chars of an ssh fingerprint", () => {
    expect(shortKey("SHA256:abcdefghijklmnopqrstuvwxyz")).toBe("SHA256:abcdefghijkl");
  });
  it("leaves short keys alone", () => {
    expect(shortKey("ABCD")).toBe("ABCD");
  });
});
