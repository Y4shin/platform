import { expect, test } from '@junius/e2e';

// "No hardcoded strings" DOM-level enforcement. The pseudo locale runs every
// wrapped string through a diacritic transform (a→à, e→ē, h→ĥ, …); any visible
// UI text that's still plain ASCII Latin words is, by definition, not wrapped
// in a `t` / `<Trans>` macro and therefore wouldn't translate to German either.
//
// The lightweight catalog-completeness companion lives in
// `plugins/events/frontend/src/i18n.test.ts` — that one runs in vitest and
// proves Lingui's pipeline produced non-ASCII entries for every msgid.
//
// Gated behind JUNIUS_E2E because it needs the dev stack. The session is
// fixture-seeded (no Authentik round-trip).
test.describe('pseudo locale', () => {
  test.skip(!process.env.JUNIUS_E2E, 'set JUNIUS_E2E=1 against a running dev stack');

  test('every visible string on /p/events is pseudo-localised', async ({ page, loginAs }) => {
    await loginAs('alice', { permissions: ['events:read'] });
    await page.goto('/p/events');
    await page.getByRole('combobox', { name: /Locale|Sprache/i }).selectOption('pseudo');

    // The list-page header would be "Events" under en, "Veranstaltungen" under
    // de, and a diacritic-heavy form under pseudo. Any plain "Events" surviving
    // the switch means that string skipped extraction.
    const headerText = await page.getByRole('heading', { level: 1 }).first().textContent();
    expect(headerText, 'header should be pseudo-localised').not.toMatch(/^[\x20-\x7E]+$/);

    // Walk every text-bearing element in the main content region; any element
    // whose visible text is *all ASCII Latin word characters* with no
    // diacritics is suspect. Numbers, punctuation, code spans (e.g. URLs),
    // and user-entered event data are excluded.
    const offenders = await page.locator('main').evaluate((root) => {
      const ASCII_LATIN_ONLY = /^[\x20-\x7E]*$/;
      const HAS_LATIN_LETTERS = /[A-Za-z]/;
      const ALLOWED_TAGS = new Set(['CODE', 'PRE']);
      const out: string[] = [];
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      let node: Node | null;
      while ((node = walker.nextNode()) !== null) {
        const text = node.textContent ?? '';
        const trimmed = text.trim();
        if (!trimmed) continue;
        if (!HAS_LATIN_LETTERS.test(trimmed)) continue;
        if (!ASCII_LATIN_ONLY.test(trimmed)) continue;
        // Skip code-like content (slugs, IDs, URLs).
        const el = node.parentElement;
        if (el && ALLOWED_TAGS.has(el.tagName)) continue;
        out.push(trimmed);
      }
      return out;
    });

    expect(offenders, `unwrapped strings on /p/events:\n${offenders.join('\n')}`).toEqual([]);
  });
});
