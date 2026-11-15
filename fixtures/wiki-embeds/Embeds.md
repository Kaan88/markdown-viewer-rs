# Wiki labels and embeds

Needs `Target.md` beside it and `../assets/badge.png`; no folder has to be open.

## Markup in a label

- [[Target|a *b* c]] has emphasis inside one link to Target.
- [[Target|**bold** and `code` words]] has strong and code.
- [[Target#Open|the *open* part]] points at a heading and still shows the label.
- [[Target|plain label]] has no markup, as before.

## Embeds

An image embed shows the picture: ![[../assets/badge.png]] (as `![](../assets/badge.png)` would).

A note embed is a link to the note: ![[Target]], and with a label ![[Target|the target note]].

One that resolves to nothing stays text: ![[missing.png]] and ![[Nowhere]].
