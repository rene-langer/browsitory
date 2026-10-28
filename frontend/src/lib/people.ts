import type { SignatureInput, Trailer } from "../ipc/RepoClient";

/** `Name <email>`, as a trailer value and a chip label. */
export function formatPerson(person: SignatureInput): string {
  return `${person.name} <${person.email}>`;
}

/**
 * Parses `Name <email>`; `null` unless both parts are there: a name without `<`, `>` or line
 * breaks, and an email without `<`, `>` or whitespace (the backend checks the same).
 */
export function parsePerson(text: string): SignatureInput | null {
  const match = /^\s*([^<>\n\r]+?)\s*<([^<>\s]+)>\s*$/.exec(text);
  return match === null ? null : { name: match[1], email: match[2] };
}

export function coAuthorTrailers(people: SignatureInput[]): Trailer[] {
  return people.map((person) => ({ key: "Co-authored-by", value: formatPerson(person) }));
}

/** The same person for co-author purposes: emails compare case-insensitively. */
export function sameEmail(a: SignatureInput, b: SignatureInput): boolean {
  return a.email.toLowerCase() === b.email.toLowerCase();
}
