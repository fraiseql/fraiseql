// Shared by `greet` and `shout`: each imports it as `@shared/text.ts`.
export const greeting = (name: string): string => `hello, ${name}`;

export function shout(text: string): string {
  return `${text.toUpperCase()}!`;
}
