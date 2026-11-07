---
Status: proposed
---

# A File operation that doesn't place the file interrupts with a modal

Every transient notice in lecoq uses the status bar: a single line that auto-expires after about three seconds, with no severity, colour or dismiss button. A Copy to or Move to that did *not* place the file in the Target folder — a hard error, a name clash with a file already there, or the file already being in that folder — rode the same channel, so the outcome that most needs to be seen was the one easiest to miss (and the already-in-folder case was silent). We decided these non-success outcomes instead open a dismissable modal, carrying a message worded for the specific outcome; a successful Copy to / Move to keeps its quiet status-bar line.

The modal is surprising precisely because it is the only interrupting notice in the app, so a future reader will ask why File operations alone depart from the status bar. The trade-off: a modal cannot be missed, at the cost of interrupting the user and demanding a dismissal even for the harmless "already in that folder" no-op. We judged "cannot be missed" to be worth that cost for an action the user took deliberately on a file and expects to have landed somewhere.

Decided while designing the outcome warning for File operations, a follow-up to [Copy to / Move to](https://github.com/acastaner/lecoq/issues/53).

## Considered Options

- **Keep the status bar but make failures louder** (an error colour or icon, or hold the line until the next action). Rejected: a timed line is still missable, and a line that lingers until dismissed is a dismiss-me widget wearing the status bar's clothes — the inconsistency without the clarity.
- **Add a toast / notification system.** Rejected: lecoq has no such component, building one for three messages is disproportionate, and a toast is still a notice you can look away from and lose.
- **Modal for hard errors only; leave the name clash and the already-there cases on the status bar (or silent).** Rejected: splitting outcomes across channels is what made them easy to miss in the first place, and we want every outcome that failed to place the file to surface the same way.
