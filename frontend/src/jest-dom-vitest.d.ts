// Vitest 5 changed `Assertion<T>` to `Assertion<R, T>` and documents augmenting `Matchers<R, T>`
// for custom matchers (https://vitest.dev/guide/migration). @testing-library/jest-dom 7.0.1 still
// augments the old one-parameter `Assertion<T>`, which no longer merges, so its matcher types are
// lost (testing-library/jest-dom#738). Remove this file once a jest-dom release supports Vitest 5.
import type { TestingLibraryMatchers } from "@testing-library/jest-dom/matchers";

declare module "vitest" {
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type, @typescript-eslint/no-unused-vars
  interface Matchers<R extends void | Promise<void> = void | Promise<void>, T = unknown>
    extends TestingLibraryMatchers<unknown, R> {}
}
