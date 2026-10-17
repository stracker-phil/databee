import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useApiResult, type ApiResultOptions } from "./useApiResult";

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const appError = { kind: "database", message: "boom", code: "1064" };

function setup(initialKey: string | null, fetcher: (key: string) => Promise<string>, options?: ApiResultOptions) {
  return renderHook(({ key }) => useApiResult(key, () => fetcher(key as string), options), { initialProps: { key: initialKey } });
}

afterEach(() => vi.useRealTimers());

describe("useApiResult", () => {
  it("stays loading and does not fetch while the key is null", () => {
    const fetcher = vi.fn(() => Promise.resolve("x"));
    const { result } = setup(null, fetcher);
    expect(fetcher).not.toHaveBeenCalled();
    expect(result.current).toMatchObject({ state: "loading", data: null });
  });

  it("goes from loading to loaded with the data", async () => {
    const d = deferred<string>();
    const { result } = setup("a", () => d.promise);
    expect(result.current).toMatchObject({ state: "loading", data: null, error: null });
    await act(async () => d.resolve("A"));
    expect(result.current).toMatchObject({ state: "loaded", data: "A", error: null });
  });

  it("fails with the AppError and calls onError once", async () => {
    const onError = vi.fn();
    const { result } = setup("a", vi.fn().mockRejectedValue(appError), { onError });
    await waitFor(() => expect(result.current.state).toBe("failed"));
    expect(result.current.error).toEqual(appError);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith(appError);
  });

  describe("superseded replies", () => {
    it("keeps the latest key's data when an older reply arrives last", async () => {
      const first = deferred<string>();
      const second = deferred<string>();
      const replies: Record<string, Promise<string>> = { a: first.promise, b: second.promise };
      const { result, rerender } = setup("a", (k) => replies[k]);
      rerender({ key: "b" });
      await act(async () => second.resolve("B"));
      await act(async () => first.resolve("A"));
      expect(result.current).toMatchObject({ state: "loaded", data: "B" });
    });

    it("ignores a reply that arrives after unmount", async () => {
      const d = deferred<string>();
      const error = vi.spyOn(console, "error").mockImplementation(() => {});
      const { unmount } = setup("a", () => d.promise);
      unmount();
      await act(async () => d.resolve("A"));
      expect(error).not.toHaveBeenCalled();
      error.mockRestore();
    });
  });

  describe("keepData", () => {
    it("shows the previous data while a new key loads", async () => {
      const second = deferred<string>();
      const { result, rerender } = setup("a", (k) => (k === "a" ? Promise.resolve("A") : second.promise), { keepData: true });
      await waitFor(() => expect(result.current.data).toBe("A"));
      rerender({ key: "b" });
      expect(result.current).toMatchObject({ state: "loading", data: "A" });
    });

    it("shows the last loaded data after a failure", async () => {
      const { result, rerender } = setup("a", vi.fn().mockResolvedValueOnce("A").mockRejectedValueOnce(appError), { keepData: true });
      await waitFor(() => expect(result.current.data).toBe("A"));
      rerender({ key: "b" });
      await waitFor(() => expect(result.current.state).toBe("failed"));
      expect(result.current.data).toBe("A");
    });

    it("has no data while a new key loads without keepData", async () => {
      const second = deferred<string>();
      const { result, rerender } = setup("a", (k) => (k === "a" ? Promise.resolve("A") : second.promise));
      await waitFor(() => expect(result.current.state).toBe("loaded"));
      rerender({ key: "b" });
      expect(result.current).toMatchObject({ state: "loading", data: null });
    });
  });

  it("reload fetches the same key again", async () => {
    const second = deferred<string>();
    const fetcher = vi.fn().mockResolvedValueOnce("old").mockReturnValueOnce(second.promise);
    const { result } = setup("a", fetcher);
    await waitFor(() => expect(result.current.data).toBe("old"));
    act(() => result.current.reload());
    expect(result.current.state).toBe("loading");
    await act(async () => second.resolve("new"));
    expect(result.current).toMatchObject({ state: "loaded", data: "new" });
  });

  describe("delayMs", () => {
    it("fetches only after the delay elapses", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const fetcher = vi.fn((k: string) => Promise.resolve(k));
      const { result } = setup("a", fetcher, { delayMs: 200 });
      await act(() => vi.advanceTimersByTimeAsync(100));
      expect(fetcher).not.toHaveBeenCalled();
      await act(() => vi.advanceTimersByTimeAsync(150));
      expect(fetcher).toHaveBeenCalledTimes(1);
      expect(result.current).toMatchObject({ state: "loaded", data: "a" });
    });

    it("fetches only the last key when the key changes within the delay", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const fetcher = vi.fn((k: string) => Promise.resolve(k));
      const { result, rerender } = setup("a", fetcher, { delayMs: 200 });
      await act(() => vi.advanceTimersByTimeAsync(100));
      rerender({ key: "b" });
      await act(() => vi.advanceTimersByTimeAsync(300));
      expect(fetcher.mock.calls).toEqual([["b"]]);
      expect(result.current).toMatchObject({ state: "loaded", data: "b" });
    });
  });
});
