---
id: "{{ entry_id }}"
kind: "{{ kind }}"
author: "{{ author_id }}"
created: "{{ created_at }}"
parent: "{{ parent_entry_id_or_stem }}"
talk_for: "{{ talk_for_uri }}"
status: proposed
evidence:
  - "{{ evidence_uri }}"
---

## {{ title }}

{{ body }}

## Evidence

- {{ evidence_summary }}
