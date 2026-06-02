const punctuationNeedingSpace = /([,.;:!?])\s*(?=\p{L})/gu;
const spaceBeforePunctuation = /\s+([,.;:!?])/g;
const repeatedWhitespace = /\s+/g;
const terminalPunctuation = /[.!?]$/;

export function cleanupTranscript(transcript: string): string {
  let cleaned = transcript.trim().replace(repeatedWhitespace, " ");

  if (cleaned.length === 0) {
    return "";
  }

  cleaned = cleaned
    .replace(spaceBeforePunctuation, "$1")
    .replace(punctuationNeedingSpace, (match, punctuation, offset) => {
      const previousCharacter =
        offset > 0 ? cleaned.charAt(offset - 1) : undefined;

      if (punctuation === "." && isAsciiDigit(previousCharacter)) {
        return match;
      }

      return `${punctuation} `;
    });

  cleaned = capitalizeFirstAlphabeticalCharacter(cleaned);

  if (!terminalPunctuation.test(cleaned)) {
    cleaned = `${cleaned}.`;
  }

  return cleaned;
}

function capitalizeFirstAlphabeticalCharacter(value: string): string {
  for (const match of value.matchAll(/\p{L}/gu)) {
    const index = match.index;

    if (index === undefined) {
      continue;
    }

    return `${value.slice(0, index)}${match[0].toLocaleUpperCase()}${value.slice(
      index + match[0].length,
    )}`;
  }

  return value;
}

function isAsciiDigit(value: string | undefined): boolean {
  return value !== undefined && value >= "0" && value <= "9";
}
