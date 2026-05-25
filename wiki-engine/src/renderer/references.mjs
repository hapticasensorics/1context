import { createHash } from 'node:crypto';

export function sha256Text(value) {
  return createHash('sha256').update(String(value)).digest('hex');
}

export function slugToken(value) {
  return String(value || 'page')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    || 'page';
}

export function stableCodeBlockId(pageSlug, sequence, language, text) {
  const hash = sha256Text(`${language || ''}\n${text || ''}`).slice(0, 8);
  const ordinal = String(sequence).padStart(3, '0');
  return `code-${slugToken(pageSlug)}-${ordinal}-${hash}`;
}

export function codeCitationUri(pageId, codeId) {
  return `user-wiki://page/${encodeURIComponent(pageId || 'unknown')}/code/${encodeURIComponent(codeId)}`;
}

export function assetCitationUri(pageId, filename) {
  return `user-wiki://page/${encodeURIComponent(pageId || 'unknown')}/assets/${encodeURIComponent(filename)}`;
}

export function linkCitationUri(pageId, linkId) {
  return `user-wiki://page/${encodeURIComponent(pageId || 'unknown')}/links/${encodeURIComponent(linkId)}`;
}

export function citationCitationUri(pageId, citationId) {
  return `user-wiki://page/${encodeURIComponent(pageId || 'unknown')}/citations/${encodeURIComponent(citationId)}`;
}
