import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Field } from "./Field";

describe("Field", () => {
  it("labels the control and exposes hint as its description", () => {
    render(<Field label="Tag name" hint="No spaces">{(props) => <input {...props} />}</Field>);
    const input = screen.getByLabelText("Tag name");
    expect(input).toHaveAccessibleDescription("No spaces");
    expect(input).not.toBeInvalid();
  });

  it("marks the control invalid and describes it with the error", () => {
    render(
      <Field label="Tag name" hint="No spaces" error="Name is taken">
        {(props) => <input {...props} />}
      </Field>,
    );
    const input = screen.getByLabelText("Tag name");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription("No spaces Name is taken");
  });

  it("sets required on the control", () => {
    render(
      <Field label="Name" required>
        {(props) => <input {...props} />}
      </Field>,
    );
    expect(screen.getByLabelText(/Name/)).toBeRequired();
  });
});
