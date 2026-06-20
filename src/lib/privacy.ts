/// Centralized privacy validation for diagnostics.
/// Single source of truth for forbidden keys, patterns, and redaction rules.

export const FORBIDDEN_KEYS: ReadonlySet<string> = new Set([
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
]);

export const FORBIDDEN_PATTERNS: ReadonlyArray<{
  pattern: RegExp;
  name: string;
}> = [
  { pattern: /\bBearer\s+[A-Za-z0-9._\-+/=]{8,}/i, name: "Bearer token" },
  { pattern: /gsk_[A-Za-z0-9]{8,}/, name: "Groq API key prefix" },
  { pattern: /sk-[A-Za-z0-9]{8,}/, name: "Generic API key prefix" },
  { pattern: /sk_[A-Za-z0-9]{8,}/, name: "OpenAI API key prefix" },
  {
    pattern: /Authorization\s*[:=]/i,
    name: "Authorization header",
  },
  {
    pattern: /x-api-key\s*[:=]/i,
    name: "x-api-key header",
  },
];

export function redactValue(value: string): string {
  const lowered = value.toLowerCase();

  if (
    lowered.includes("...") ||
    lowered.includes("…") ||
    lowered.includes("****")
  ) {
    return value;
  }

  for (const marker of [
    "bearer ",
    "bearer",
    "authorization",
    "api_key",
    "api-key",
    "apikey",
    "gsk_",
    "sk-",
    "sk_",
    "clipboard_text",
    "transcript",
    "raw_audio",
    "audio_bytes",
  ]) {
    if (lowered.includes(marker)) {
      return "redacted";
    }
  }
  return value;
}

export function assertNoForbiddenKeys(value: unknown, path: string): void {
  if (value === null || value === undefined) {
    return;
  }
  if (typeof value !== "object") {
    return;
  }
  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i += 1) {
      assertNoForbiddenKeys(value[i], `${path}[${i}]`);
    }
    return;
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (FORBIDDEN_KEYS.has(key.toLowerCase())) {
      throw new Error(
        `Contains forbidden key: ${path ? `${path}.` : ""}${key}`,
      );
    }
    assertNoForbiddenKeys(record[key], path ? `${path}.${key}` : key);
  }
}

export function assertNoForbiddenPatterns(json: string): void {
  for (const { pattern, name } of FORBIDDEN_PATTERNS) {
    if (pattern.test(json)) {
      throw new Error(`Contains forbidden pattern: ${name}`);
    }
  }
}
