import path from "node:path";
import Mocha from "mocha";
import { globSync } from "node:fs";

export function run(): Promise<void> {
  const mocha = new Mocha({ ui: "bdd", timeout: 60000, color: true });
  const specsRoot = path.resolve(__dirname, "..", "specs");
  const files = globSync("**/*.spec.js", { cwd: specsRoot });
  for (const file of files) mocha.addFile(path.join(specsRoot, file));

  return new Promise((resolve, reject) => {
    mocha.run((failures) => {
      if (failures > 0) reject(new Error(`${failures} test(s) failed`));
      else resolve();
    });
  });
}
