---
paths:
  - "src/**"
description: front-matter is machine configuration, not the document's text
---

# Adversarial Markdown

A paragraph carrying `# not a heading` and a [link](https://example.com) that a
line-based classifier would misread.

<!-- TOC -->

- [Fences](#fences)
- [Tables](#tables)

<!-- /TOC -->

Setext headings count too
=========================

## Fences

A fence holds code however much the code reads like a sentence:

```php
// This comment is inside a fence and is therefore code, not prose.
$resolver = new PriceResolver();
```

An unlabelled fence is still a fence:

```
plain text inside a fence
```

    an indented code block is code as well

## Tables

| column | meaning                        |
| ------ | ------------------------------ |
| cells  | prose, escaped pipe `a \| b`   |
| pipes  | code, because they scale       |

***

<!-- rumdl-disable MD033 -->

<p align="center">markup you chose to write</p>

<!-- rumdl-enable MD033 -->

> A blockquote is prose, marker included.

1. Ordered list items are prose.
   1. So are nested ones.
