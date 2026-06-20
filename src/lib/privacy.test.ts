import { describe, expect, it } from "vitest";
import {
  FORBIDDEN_KEYS,
  FORBIDDEN_PATTERNS,
  assertNoForbiddenKeys,
  assertNoForbiddenPatterns,
  redactValue,
} from "./privacy";

describe("redactValue", () => {
  it("preserves already-masked values", () => {
    expect(redactValue("gsk_…****")).toBe("gsk_…****");
    expect(redactValue("sk-...abcd")).toBe("sk-...abcd");
    expect(redactValue("****...****")).toBe("****...****");
  });

  it("redacts values containing bearer tokens", () => {
    expect(redactValue("Bearer gsk_abc123")).toBe("redacted");
  });

  it("redacts values containing API key patterns", () => {
    expect(redactValue("gsk_abc123def456")).toBe("redacted");
    expect(redactValue("sk-abc123def456")).toBe("redacted");
    expect(redactValue("api_key=secret")).toBe("redacted");
    expect(redactValue("api-key=secret")).toBe("redacted");
  });

  it("redacts values containing transcript references", () => {
    expect(redactValue("transcript content here")).toBe("redacted");
    expect(redactValue("clipboard_text value")).toBe("redacted");
  });

  it("redacts values containing raw audio references", () => {
    expect(redactValue("raw_audio data")).toBe("redacted");
    expect(redactValue("audio_bytes buffer")).toBe("redacted");
  });

  it("preserves normal diagnostic strings", () => {
    expect(redactValue("timeout_error")).toBe("timeout_error");
    expect(redactValue("server_error")).toBe("server_error");
    expect(redactValue("rate_limit")).toBe("rate_limit");
    expect(redactValue("invalid_request")).toBe("invalid_request");
  });

  it("is case-insensitive", () => {
    expect(redactValue("BEARER token")).toBe("redacted");
    expect(redactValue("GSK_abc123")).toBe("redacted");
    expect(redactValue("TRANSCRIPT data")).toBe("redacted");
  });
});

describe("assertNoForbiddenKeys", () => {
  it("accepts an object without forbidden keys", () => {
    const obj = {
      app: "Floe",
      version: "1.0.0",
      platform: { os: "macos" },
      pipeline: { total_ms: 100 },
    };
    expect(() => assertNoForbiddenKeys(obj, "")).not.toThrow();
  });

  it("rejects objects with 'transcript' key at any depth", () => {
    const obj = {
      app: "Floe",
      last_session: {
        transcript: "leaked text",
      },
    };
    expect(() => assertNoForbiddenKeys(obj, "")).toThrow(
      /forbidden key.*transcript/i,
    );
  });

  it("rejects objects with 'api_key' key", () => {
    const obj = { api_key: "gsk_secret" };
    expect(() => assertNoForbiddenKeys(obj, "")).toThrow(
      /forbidden key.*api_key/i,
    );
  });

  it("rejects objects with 'wav' key", () => {
    const obj = { wav: "base64data" };
    expect(() => assertNoForbiddenKeys(obj, "")).toThrow(/forbidden key.*wav/i);
  });

  it("rejects objects with 'clipboard' key", () => {
    const obj = { clipboard: "sensitive" };
    expect(() => assertNoForbiddenKeys(obj, "")).toThrow(
      /forbidden key.*clipboard/i,
    );
  });

  it("rejects objects with 'text' key at any depth", () => {
    const obj = {
      data: {
        inner: {
          text: "some content",
        },
      },
    };
    expect(() => assertNoForbiddenKeys(obj, "")).toThrow(
      /forbidden key.*text/i,
    );
  });

  it("accepts null and undefined values", () => {
    expect(() => assertNoForbiddenKeys(null, "")).not.toThrow();
    expect(() => assertNoForbiddenKeys(undefined, "")).not.toThrow();
  });

  it("handles arrays by checking each element", () => {
    const arr = [
      { name: "stage1" },
      { name: "stage2", text: "should be caught" },
    ];
    expect(() => assertNoForbiddenKeys(arr, "")).toThrow(
      /forbidden key.*text/i,
    );
  });

  it("is case-insensitive for key names", () => {
    const obj = { Transcript: "leaked" };
    expect(() => assertNoForbiddenKeys(obj, "")).toThrow(
      /forbidden key.*Transcript/i,
    );
  });
});

describe("assertNoForbiddenPatterns", () => {
  it("accepts clean JSON strings", () => {
    const json = JSON.stringify({
      app: "Floe",
      version: "1.0.0",
      error_code: "timeout",
    });
    expect(() => assertNoForbiddenPatterns(json)).not.toThrow();
  });

  it("rejects JSON containing Bearer tokens", () => {
    const json = JSON.stringify({
      error: "Authorization: Bearer gsk_secret123",
    });
    expect(() => assertNoForbiddenPatterns(json)).toThrow(
      /forbidden pattern.*Bearer token/i,
    );
  });

  it("rejects JSON containing Groq API keys", () => {
    const json = JSON.stringify({
      key: "gsk_abc123def456ghi789",
    });
    expect(() => assertNoForbiddenPatterns(json)).toThrow(
      /forbidden pattern.*Groq API key/i,
    );
  });

  it("rejects JSON containing OpenAI API keys (sk-)", () => {
    const json = JSON.stringify({
      key: "sk-abcdefghijklmnop",
    });
    expect(() => assertNoForbiddenPatterns(json)).toThrow(
      /forbidden pattern.*Generic API key/i,
    );
  });

  it("rejects JSON containing sk_ format keys", () => {
    const json = JSON.stringify({
      key: "sk_abc123def456",
    });
    expect(() => assertNoForbiddenPatterns(json)).toThrow(
      /forbidden pattern.*OpenAI API key/i,
    );
  });

  it("rejects JSON containing Authorization headers", () => {
    const json = JSON.stringify({
      header: "Authorization=Bearer xyz",
    });
    expect(() => assertNoForbiddenPatterns(json)).toThrow(
      /forbidden pattern.*Authorization/i,
    );
  });
});

describe("FORBIDDEN_KEYS set", () => {
  it("contains all expected forbidden key names", () => {
    const expected = [
      "transcript",
      "transcripts",
      "transcript_text",
      "cleaned",
      "cleaned_text",
      "text",
      "api_key",
      "apikey",
      "api-key",
      "key",
      "bearer",
      "authorization",
      "auth",
      "samples",
      "raw_audio",
      "rawaudio",
      "audio_data",
      "audiodata",
      "audio_bytes",
      "audiobytes",
      "wav",
      "wav_bytes",
      "wavbytes",
      "pcm",
      "pcm_samples",
      "pcmsamples",
      "clipboard",
      "clipboard_text",
      "clipboardtext",
      "response",
      "response_body",
      "responsebody",
      "body",
      "payload",
      "headers",
      "request",
      "url",
      "endpoint",
    ];
    for (const key of expected) {
      expect(FORBIDDEN_KEYS.has(key)).toBe(true);
    }
    // No extra key that shouldn't be there
    expect(FORBIDDEN_KEYS.size).toBe(expected.length);
  });
});

describe("FORBIDDEN_PATTERNS", () => {
  it("includes Bearer token pattern", () => {
    const bearer = FORBIDDEN_PATTERNS.find((p) => p.name === "Bearer token");
    expect(bearer).toBeDefined();
    expect(bearer!.pattern.test("Bearer gsk_abcdefgh")).toBe(true);
    expect(bearer!.pattern.test("bearer token1234")).toBe(true);
    expect(bearer!.pattern.test("no match")).toBe(false);
  });

  it("includes Groq API key pattern", () => {
    const groq = FORBIDDEN_PATTERNS.find(
      (p) => p.name === "Groq API key prefix",
    );
    expect(groq).toBeDefined();
    expect(groq!.pattern.test("gsk_abcdef12")).toBe(true);
    expect(groq!.pattern.test("no match")).toBe(false);
    // Also confirm short keys are NOT falsely flagged
    expect(groq!.pattern.test("gsk_abc123")).toBe(false);
  });

  it("includes OpenAI API key pattern (sk_)", () => {
    const openai = FORBIDDEN_PATTERNS.find(
      (p) => p.name === "OpenAI API key prefix",
    );
    expect(openai).toBeDefined();
    expect(openai!.pattern.test("sk_abcdef12")).toBe(true);
    expect(openai!.pattern.test("no match")).toBe(false);
  });
});
