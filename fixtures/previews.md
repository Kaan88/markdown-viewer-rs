# Hover previews

Rest the pointer on a footnote mark[^one] and a card shows its text. The same goes for a link to
a heading of this page, like [the notes below](#notes), and for a link to another Markdown file,
like [the linked file](previews-target.md). A [web address](https://example.com/page) keeps its
hint at the bottom left.

A second footnote[^two] has a longer text, to show the card cut at a few lines.

## Notes

The notes are a heading with a few lines under it. This is the first paragraph, short enough to
read in a glance.

A second paragraph follows, and the card shows the lines of both until it has no more room, then
ends with an ellipsis where the text goes on.

- a list item
- another one

### Deeper

Never shown in the card above, because a heading ends it.

[^one]: A short note, in a single line.
[^two]: The second note is long on purpose. It goes on for several sentences so that the card has to wrap it over many lines, cut it at the limit and end the last line with an ellipsis instead of growing without end. Nothing past the limit is ever read in the card, though the page lists the whole text at its end. This sentence is here only to make the note longer still, so that even a wide card runs out of room before the note runs out of words, and the last line of the card shows that with its ellipsis.
