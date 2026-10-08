/**
 * vue-i18n plural rule for Czech message choices `one | few | many`
 * (1 zájem | 2–4 zájmy | 0, 5+ zájmů). Messages with two choices fall back to `one | other`.
 */
export function czechPlural(choice: number, choicesLength: number): number {
  const n = Math.abs(choice)
  if (choicesLength < 3) return n === 1 ? 0 : 1
  if (n === 1) return 0
  if (n >= 2 && n <= 4) return 1
  return 2
}
