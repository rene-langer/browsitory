import { describe, expect, it } from "vitest";
import { cascadedValue, compoundsOf, declarationOf, type Rule, rulesOf, subjectOf } from "../test/cssCascade";
import formDialogCss from "./primitives/FormDialog.module.css?raw";
import fieldCss from "./primitives/Field.module.css?raw";
import branchTreeCss from "./BranchTree.module.css?raw";
import cloneCss from "./CloneDialog.module.css?raw";
import cloneTsx from "./CloneDialog.tsx?raw";
import createTagTsx from "./CreateTagDialog.tsx?raw";
import deleteRemoteTagTsx from "./DeleteRemoteTagDialog.tsx?raw";
import identityCss from "./IdentityDialog.module.css?raw";
import identityTsx from "./IdentityDialog.tsx?raw";
import mergeCss from "./MergeDialog.module.css?raw";
import mergeTsx from "./MergeDialog.tsx?raw";
import parentPickerCss from "./ParentPickerDialog.module.css?raw";
import parentPickerTsx from "./ParentPickerDialog.tsx?raw";
import preferencesCss from "./PreferencesDialog.module.css?raw";
import preferencesTsx from "./PreferencesDialog.tsx?raw";
import pullDivergedCss from "./PullDivergedDialog.module.css?raw";
import pullDivergedTsx from "./PullDivergedDialog.tsx?raw";
import pushBranchCss from "./PushBranchDialog.module.css?raw";
import pushBranchTsx from "./PushBranchDialog.tsx?raw";
import resetCss from "./ResetDialog.module.css?raw";
import resetTsx from "./ResetDialog.tsx?raw";
import stashCss from "./StashDialog.module.css?raw";
import stashTsx from "./StashDialog.tsx?raw";
import switchConflictCss from "./SwitchConflictDialog.module.css?raw";
import switchConflictTsx from "./SwitchConflictDialog.tsx?raw";

// jsdom lays nothing out and Vitest stubs module CSS, so the cascade is checked on the rules
// (#972, as #970's test does for the user commands editor). FormDialog lays every label in a
// dialog out as a column, for a field's name above its input; a dialog's checkbox or radio label
// has its own class for a row, and that class must win.

// FormDialog's rules for a label inside the dialog.
const formDialogLabelRules: Rule[] = rulesOf(formDialogCss).filter(
  ({ selector }) => subjectOf(selector) === "label" && compoundsOf(selector).includes(".dialog"),
);

// A stylesheet's rules for a label with `className`.
const classRules = (css: string, className: string): Rule[] =>
  rulesOf(css).filter(({ selector }) => subjectOf(selector) === `.${className}`);

// What a label with `className` (or none) inside a FormDialog gets for `property`; the initial
// value when no rule declares it.
function labelValue(property: string, css?: string, className?: string): string | null {
  const own = css && className ? classRules(css, className) : [];
  return cascadedValue([...formDialogLabelRules, ...own], property);
}

const checkboxAndRadioLabels = [
  { dialog: "MergeDialog", tsx: mergeTsx, css: mergeCss, className: "choice" },
  { dialog: "SwitchConflictDialog", tsx: switchConflictTsx, css: switchConflictCss, className: "choice" },
  { dialog: "IdentityDialog", tsx: identityTsx, css: identityCss, className: "choice" },
  { dialog: "ParentPickerDialog", tsx: parentPickerTsx, css: parentPickerCss, className: "parent" },
  { dialog: "PullDivergedDialog", tsx: pullDivergedTsx, css: pullDivergedCss, className: "remember" },
  { dialog: "ResetDialog", tsx: resetTsx, css: resetCss, className: "mode" },
  { dialog: "StashDialog", tsx: stashTsx, css: stashCss, className: "option" },
  { dialog: "PushBranchDialog", tsx: pushBranchTsx, css: pushBranchCss, className: "checkbox" },
  { dialog: "CreateTagDialog", tsx: createTagTsx, css: pushBranchCss, className: "checkbox" },
  { dialog: "DeleteRemoteTagDialog", tsx: deleteRemoteTagTsx, css: pushBranchCss, className: "checkbox" },
  { dialog: "PreferencesDialog", tsx: preferencesTsx, css: preferencesCss, className: "checkbox" },
  { dialog: "CloneDialog", tsx: cloneTsx, css: cloneCss, className: "check" },
];

describe("FormDialog's labels (#972)", () => {
  it("stacks a label as a column: its name above its input", () => {
    expect(formDialogLabelRules.length).toBeGreaterThan(0);
    expect(labelValue("display")).toBe("flex");
    expect(labelValue("flex-direction")).toBe("column");
    expect(labelValue("gap")).toBe("2px");
  });

  it.each(checkboxAndRadioLabels)("$dialog puts each .$className input beside its text", ({ tsx, css, className }) => {
    // The dialog's checkbox or radio labels carry the class.
    expect(tsx).toMatch(new RegExp(`<label[^>]*className=\\{styles\\.${className}\\}`));
    expect(classRules(css, className).length).toBeGreaterThan(0);

    expect(["flex", "inline-flex"]).toContain(labelValue("display", css, className));
    expect(labelValue("flex-direction", css, className) ?? "row").toBe("row");
    // The class's spacing between input and text wins over the column's 2px, too.
    const ownGap = classRules(css, className)
      .map(({ body }) => declarationOf(body, "gap")?.value)
      .find(Boolean);
    if (ownGap) expect(labelValue("gap", css, className)).toBe(ownGap);
  });

  it("needs no !important in a dialog's label class to win over FormDialog", () => {
    for (const { dialog, css, className } of checkboxAndRadioLabels) {
      for (const { body } of classRules(css, className)) {
        expect({ dialog, className, body }).toEqual({ dialog, className, body: body.replace(/\s*!\s*important/g, "") });
      }
    }
  });

  it.each([
    { name: "Field's name", css: fieldCss, className: "label" },
    { name: "the remote dialogs' fields", css: branchTreeCss, className: "label" },
    { name: "Clone's URL field", css: cloneCss, className: "grow" },
  ])("keeps $name (.$className) a column", ({ css, className }) => {
    expect(classRules(css, className).length).toBeGreaterThan(0);
    expect(labelValue("display", css, className)).toBe("flex");
    expect(labelValue("flex-direction", css, className)).toBe("column");
  });
});
