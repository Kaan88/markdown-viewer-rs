# HTML extras

Three HTML elements GitHub styles: ruby annotations, description lists and figures.

## Ruby

A short one: <ruby>漢<rt>kan</rt>字<rt>ji</rt></ruby> reads as two characters, and with the fallback parentheses a browser without ruby support would show, <ruby>東<rp>(</rp><rt>tou</rt><rp>)</rp></ruby> shows no parentheses here.

A ruby in the middle of a wrapped paragraph: the quick brown fox jumps over the lazy dog while the <ruby>BSOD<rt>Blue Screen Of Death</rt></ruby> annotation is wider than its base, and then <ruby>日本語<rt>にほんご</rt></ruby> follows with **bold <ruby>強<rt>きょう</rt></ruby> text** and a second line of ordinary words so that the paragraph wraps more than once at the reading width.

The line above this one must not touch the annotation. Selecting from "fox" to "follows" and copying gives the base and the annotation together.

## Description list

<dl>
<dt>Plain term</dt>
<dd>A definition under it, indented from the term.</dd>
<dt><em>Italic</em> and <code>code</code> in a term</dt>
<dd>A definition with <strong>strong</strong> text, a <a href="https://example.com">link</a>, and enough words that it wraps onto a second line at the reading width of the page.</dd>
<dd>A second definition for the same term.</dd>
<dt>Last term</dt>
<dd>

Markdown **inside** a definition, with a list:

- one
- two

</dd>
</dl>

A paragraph after the list.

## Figure

<figure>
<img src="assets/badge.png" alt="mdview badge">
<figcaption>Figure 1: the badge, left aligned as GitHub draws a figure, with a <em>caption</em> in muted, smaller text.</figcaption>
</figure>

<figure align="center">
<img src="assets/badge.png" alt="mdview badge">
<figcaption>Figure 2: the same figure with <code>align="center"</code>.</figcaption>
</figure>

Text after the figures.
