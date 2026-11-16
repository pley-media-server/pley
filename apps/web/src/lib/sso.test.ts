import { describe, expect, it } from "vitest";
import { atPublicAddress, ssoNotice, ssoStartUrl } from "./sso";

describe("ssoNotice", () => {
  it("says what happened and names the provider", () => {
    expect(ssoNotice(null, "Authentik")).toBeNull();
    expect(ssoNotice("linked", "Authentik")).toEqual({
      tone: "success",
      text: "Your Authentik account is linked: you can sign in with it now.",
    });
    expect(ssoNotice("unknown", "Authentik")?.text).toContain("link it on your account page");
    expect(ssoNotice("group", "Authentik")).toMatchObject({ tone: "error" });
    expect(ssoNotice("two_step", "Authentik")?.text).toContain("Sign in with your password");
    expect(ssoNotice("busy", "Authentik")?.text).toContain("Try again in a few minutes");
  });

  it("has an answer for codes it doesn't know", () => {
    expect(ssoNotice("something-new", "Authentik")).toEqual({ tone: "error", text: "Something went wrong. Try again." });
  });
});

describe("ssoStartUrl", () => {
  it("comes back to the page asked for", () => {
    expect(ssoStartUrl("https://pley.example.com/api/auth/sso/start", "/library/x?sort=year")).toBe(
      "https://pley.example.com/api/auth/sso/start?next=%2Flibrary%2Fx%3Fsort%3Dyear",
    );
  });
});

describe("atPublicAddress", () => {
  it("compares origins", () => {
    expect(atPublicAddress("https://pley.example.com", "https://pley.example.com")).toBe(true);
    expect(atPublicAddress("https://pley.example.com", "http://192.168.1.20:7539")).toBe(false);
    expect(atPublicAddress(undefined, "http://192.168.1.20:7539")).toBe(true);
  });
});
