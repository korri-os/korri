// The nested policy's URL validator, extracted from legacy setting-policy.ts.
export function validateNullableRetroArchHttpsUrl(
  value: string | null | undefined,
  label: string,
): string | undefined {
  if (value === undefined || value === null) return undefined

  let parsed: URL
  try {
    parsed = new URL(value)
  } catch {
    return `${label} must be an https URL`
  }

  if (parsed.protocol !== "https:") {
    return `${label} must be an https URL`
  }

  return undefined
}
