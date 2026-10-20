import { ApiError, apiErrorFromResponse, retryDelay, shouldRetry, toApiError } from "./errors";

function body(code: string, extra: Record<string, unknown> = {}) {
  return { error: { code, message: "m", retryable: false, request_id: "req_1", ...extra } };
}

describe("toApiError", () => {
  it("reads every field of an ErrorBody", () => {
    const e = toApiError(422, body("image_quality", { retryable: true, details: { hint: "tap_to_focus" } }));
    expect(e).toBeInstanceOf(ApiError);
    expect(e).toMatchObject({
      code: "image_quality",
      message: "m",
      retryable: true,
      details: { hint: "tap_to_focus" },
      requestId: "req_1",
      status: 422,
      retryAfterS: undefined,
    });
  });

  it("turns a body that isn't an ErrorBody into a retryable internal error", () => {
    for (const b of [undefined, "<html>502</html>", { error: "nope" }, body("not_a_code")]) {
      expect(toApiError(502, b)).toMatchObject({ code: "internal", retryable: true, status: 502 });
    }
  });

  it("reads a proxy's HTML response", async () => {
    const e = await apiErrorFromResponse(new Response("<html>Bad gateway</html>", { status: 502 }));
    expect(e).toMatchObject({ code: "internal", retryable: true, status: 502 });
  });
});

describe("retry policy", () => {
  const err = (status: number, code: string, extra: Record<string, unknown> = {}) =>
    toApiError(status, body(code, extra));

  it("retries request_in_progress and rate_limited after retry_after_s", () => {
    for (const e of [err(409, "request_in_progress", { retryable: true, retry_after_s: 2 }), err(429, "rate_limited", { retryable: true, retry_after_s: 7 })]) {
      expect(shouldRetry(0, e)).toBe(true);
      expect(retryDelay(0, e)).toBe(e.retryAfterS! * 1000);
    }
  });

  it("retries retryable 5xx with backoff, at most 3 times", () => {
    const e = err(500, "internal", { retryable: true });
    expect([0, 1, 2, 3].map((n) => shouldRetry(n, e))).toEqual([true, true, true, false]);
    expect([0, 1, 2].map((n) => retryDelay(n, e))).toEqual([1000, 2000, 4000]);
    expect(retryDelay(0, err(503, "upstream_unavailable", { retryable: true, retry_after_s: 5 }))).toBe(5000);
  });

  it("never retries budget_exceeded, other 4xx or a non-retryable 5xx", () => {
    expect(shouldRetry(0, err(429, "budget_exceeded"))).toBe(false);
    expect(shouldRetry(0, err(400, "invalid_request"))).toBe(false);
    expect(shouldRetry(0, err(422, "image_quality", { retryable: true }))).toBe(false);
    expect(shouldRetry(0, err(404, "not_found"))).toBe(false);
    expect(shouldRetry(0, err(500, "internal"))).toBe(false);
  });

  it("retries network errors but not aborts", () => {
    expect(shouldRetry(0, new TypeError("Network request failed"))).toBe(true);
    expect(shouldRetry(0, Object.assign(new Error("aborted"), { name: "AbortError" }))).toBe(false);
  });
});
