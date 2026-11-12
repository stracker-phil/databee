/** WCAG contrast of the schemes' color pairs, for the scheme audit in `contrast.test.ts`. */

export type Rgb = [number, number, number];

export function hexToRgb(hex: string): Rgb {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? [...h].map((c) => c + c).join("") : h;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16) / 255) as Rgb;
}

const toLinear = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

function luminance(rgb: Rgb): number {
  const [r, g, b] = rgb.map(toLinear);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The WCAG contrast ratio, 1 to 21. */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** `color` at `alpha` over `bg`. */
export function over(color: Rgb, alpha: number, bg: Rgb): Rgb {
  return color.map((c, i) => c * alpha + bg[i] * (1 - alpha)) as Rgb;
}

/** The hex custom properties (`--name: #rrggbb;`) a scheme file declares. */
export function hexTokens(css: string): Record<string, string> {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return Object.fromEntries([...text.matchAll(/^\s*(--[\w-]+)\s*:\s*(#[0-9a-fA-F]{3,6})\s*;/gm)].map((m) => [m[1], m[2]]));
}

/** The share of `--text` a derived text level keeps in a scheme file (`--o2-mix: 68%;` for `--o2`), as 0 to 1. */
export function textMixAlpha(schemeCss: string, token: string): number {
  const text = schemeCss.replace(/\/\*[\s\S]*?\*\//g, "");
  const match = text.match(new RegExp(`^\\s*${token}-mix\\s*:\\s*(\\d+(?:\\.\\d+)?)%\\s*;`, "m"));
  if (!match) throw new Error(`${token}-mix is not declared as a percentage in the scheme`);
  return Number(match[1]) / 100;
}
