// Inert synthetic TypeScript source (issue #43 fixture).
// The #43 kernel never parses this text; the #44 scanner owns symbol
// discovery. Every identifier is invented fixture vocabulary.
export function fixtureGreeting(subject: string): string {
  return `hello ${subject} from the synthetic fixture`;
}

export const FIXTURE_LIMIT = 3;
