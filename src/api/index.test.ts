import { describe, expect, it } from "vitest";
import { toAppError } from "./index";

describe("toAppError", () => {
  it.each(["database", "password_required", "confirmation_required", "io"])("keeps an error of the known kind %s as is", (kind) => {
    const error = { kind, message: "boom", code: null, account: null };
    expect(toAppError(error)).toBe(error);
  });

  it("keeps the reason of a confirmation_required error", () => {
    const error = { kind: "confirmation_required", message: "needs confirmation: DROP TABLE", code: null, account: null, reason: "DROP TABLE" };
    expect(toAppError(error)).toMatchObject({ kind: "confirmation_required", reason: "DROP TABLE" });
  });

  it("falls back to an io error with the same message for an unknown kind", () => {
    expect(toAppError({ kind: "query_failed", message: "Deadlock", code: "1213" })).toMatchObject({ kind: "io", message: "Deadlock" });
  });

  it("wraps a thrown Error as an io error with its message", () => {
    expect(toAppError(new Error("socket closed"))).toEqual({ kind: "io", message: "socket closed", code: null, account: null });
  });
});
