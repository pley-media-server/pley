import { describe, expect, it } from "vitest";
import { ApiError } from "@/api/client";
import { testSetup, testUser } from "@/test/render";
import { codeKind, groupSecret, isTwoStepError, mustSetUpTwoStep, recoveryCodesText, twoStepSetupOf } from "./twoStep";

describe("two-step sign-in helpers", () => {
  it("knows the server asking for the second step", () => {
    expect(isTwoStepError(new ApiError(401, "two_step", "enter the 6-digit code"))).toBe(true);
    expect(isTwoStepError(new ApiError(401, "unauthorized", "wrong password"))).toBe(false);
    expect(isTwoStepError(new Error("two_step"))).toBe(false);
  });

  it("reads the secret of a sign-in that sets it up first", () => {
    const answer = { code: "two_step_setup", message: "set it up", setup: testSetup };
    expect(twoStepSetupOf(new ApiError(401, "two_step_setup", "set it up", answer))).toEqual(testSetup);
    expect(twoStepSetupOf(new ApiError(401, "two_step_setup", "set it up", { code: "two_step_setup" }))).toBeNull();
    expect(twoStepSetupOf(new ApiError(401, "two_step", "enter the code", answer))).toBeNull();
    expect(twoStepSetupOf(new ApiError(403, "two_step_setup", "set it up", answer))).toBeNull();
    expect(twoStepSetupOf(new Error("two_step_setup"))).toBeNull();
  });

  it("knows who sets it up at their next sign-in", () => {
    const required = { name: "Den", version: "1.0.0", setup_required: false, two_step_required: true };
    expect(mustSetUpTwoStep(testUser, required)).toBe(true);
    expect(mustSetUpTwoStep({ ...testUser, two_step: true }, required)).toBe(false);
    expect(mustSetUpTwoStep({ ...testUser, has_password: false }, required)).toBe(false);
    expect(mustSetUpTwoStep(testUser, { ...required, two_step_required: false })).toBe(false);
    expect(mustSetUpTwoStep(testUser, { name: "Den", version: "0.9.0", setup_required: false })).toBe(false);
    expect(mustSetUpTwoStep(testUser, undefined)).toBe(false);
  });

  it("tells codes from the app from recovery codes, as people type them", () => {
    expect(codeKind("123456")).toBe("app");
    expect(codeKind("123 456")).toBe("app");
    expect(codeKind("12345")).toBeNull();
    expect(codeKind("7k2m-q9xd-4hpt-3vwr")).toBe("recovery");
    expect(codeKind("7K2M Q9XD 4HPT 3VWR")).toBe("recovery");
    expect(codeKind("7k2m-q9xd")).toBeNull();
  });

  it("groups secrets in fours", () => {
    expect(groupSecret("JBSWY3DPEHPK3PXP")).toBe("JBSW Y3DP EHPK 3PXP");
    expect(groupSecret("ABCDEF")).toBe("ABCD EF");
  });

  it("writes the codes down with what they're for", () => {
    const text = recoveryCodesText(["aaaa-bbbb-cccc-dddd", "eeee-ffff-gggg-hhhh"], "bob", "Living room");
    expect(text.split("\n")).toEqual([
      "Pley recovery codes for bob on Living room",
      "Each code signs in once, instead of a code from your authenticator app.",
      "",
      "aaaa-bbbb-cccc-dddd",
      "eeee-ffff-gggg-hhhh",
      "",
    ]);
  });
});
