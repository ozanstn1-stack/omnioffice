import { describe, expect, it } from "vitest";

import { protectionProblem, type ProtectionChoice } from "./protection-guard";

const all = {
  userPassword: "user",
  ownerPassword: "owner",
  allowPrinting: true,
  allowCopying: true,
  allowEditing: true,
  allowCommenting: true,
};

const choice = (patch: Partial<ProtectionChoice>): ProtectionChoice => ({ ...all, ...patch });

describe("protectionProblem", () => {
  it("accepts a request that allows everything, whatever the passwords are", () => {
    expect(protectionProblem(choice({ allowCopying: true }))).toBeNull();
    expect(protectionProblem(choice({ ownerPassword: "", userPassword: "same" }))).toBeNull();
  });

  it("asks for an owner password when a permission is denied", () => {
    expect(protectionProblem(choice({ allowCopying: false, ownerPassword: "" }))).toBe(
      "security.ownerRequiredForRestrictions",
    );
  });

  it("refuses an owner password that matches the user password", () => {
    expect(protectionProblem(choice({ allowPrinting: false, ownerPassword: "same", userPassword: "same" }))).toBe(
      "security.ownerMustDiffer",
    );
  });

  it("accepts a separate owner password with restrictions", () => {
    expect(protectionProblem(choice({ allowEditing: false }))).toBeNull();
  });
});
