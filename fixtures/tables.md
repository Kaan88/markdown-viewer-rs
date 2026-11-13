# Tables

A narrow table is as wide as its content:

| Key | Value |
| --- | ---: |
| a | 1 |
| bb | 22 |

A wide table fills the column and wraps, each column between its longest word and its longest line:

| Option | Default | What it does |
| --- | :---: | --- |
| `page_width` | narrow | How wide the column of text may grow before the margins take the rest of the window |
| `edit_tasks` | 0 | Whether a click on a task box may change the file without asking first |
| `zoom` | 100 | The size of everything on the page, in percent |

Centred by `align`, under its caption, with a cell spanning two columns:

<table align="center">
<caption>Plan and actual, by quarter</caption>
<tr><th>Quarter</th><th>Plan</th><th>Actual</th></tr>
<tr><td>Q1</td><td>10</td><td>12</td></tr>
<tr><td>Q2</td><td colspan="2">A note that is wider than the two columns under it</td></tr>
<tr><td rowspan="2">H2</td><td>20</td><td>19</td></tr>
<tr><td>24</td><td>25</td></tr>
</table>

Stated widths: a table of 60%, a column of 200 px, a column of 50%:

<table width="60%">
<tr><th>Name</th><th>Size</th></tr>
<tr><td>readme.md</td><td>4 KB</td></tr>
</table>

<table>
<tr><td width="200">200 px</td><td>auto</td><td>auto, longer</td></tr>
</table>

<table>
<tr><td width="50%">half</td><td>the rest</td></tr>
</table>

- In a list item:

  | Flag | Meaning |
  | --- | --- |
  | `-v` | Verbose |
  | `-q` | Quiet |

- And a wide one, which wraps inside the item:

  | Flag | Meaning |
  | --- | --- |
  | `--no-session` | Neither reads nor writes the session and skips the single-instance hand-over, so a test leaves the settings alone |

> In a quote:
>
> | Left | Right |
> | :--- | ---: |
> | one | two |

A word longer than its column breaks inside the cell:

| Path | Hash | Note |
| --- | --- | --- |
| `C:\Users\someone\AppData\Local\Programs\mdview\resources\very-long-file-name.md` | 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 | unchanged |
