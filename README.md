# 🦖 Rawr!

⚠️ This is a **pet-project** that makes zero stability guarantees.

> This project will remain in beta for as long as AO3 does, which will be until
> the heat death of the universe.

## What
Command-line tool for managing HTML downloads from [AO3](https://archiveofourown.org).
- **Organize downloaded works** into a central library, automatically renaming
  files and sorting into folders based on fandom/series/title, etc.
- **Track multiple versions** of the same work (chapter updates), because some
  authors decide to replace entire fics with messages about how they "found God"
  instead of orphaning.
- **Automatic compression** because 150 million words across 5k+ fics is
  approximately 200MB. In today's cloud storage terminology that's called "free".
- **Back up** to any S3-compatible cloud storage, because computers are fragile
  and what are you talking about "you shouldn't have been messing around with
  system files"?
- **Export to PDF** with custom styling because GOOD LORD the PDF downloads that
  AO3 provides are _ugly_.

## Why
You're having the most wonderful day and you settle down for the evening only to find:

```
This has been deleted, sorry!
(Deleted work, last visited 05 Aug 2024)
```

**tl;dr:**
See a fic? Download it.
Fic got updated? Download again.

#### Why is the tool called `rawr`?
This is a tool to manage AO3 downloads, so I wanted to come up with a name that
was (a) related to the Archive, and (b) as ridiculous as the tags you depraved
lunatics come up with.

- I thought of calling it "Dead Dove", but `dd` is already a well-known CLI tool.
- As a **R**ust-based **Ar**chiver tool, `rar` unfortunately might cause
  flashbacks for people who never paid their WinRAR license.
- So I changed the pronunciation: imagine it being spoken by an Emo/Scene kid from
  the mid-2000's who was trying to unironically do an impression of a Tyrannosaurus
  Rex that had somehow magically gotten into Kawaii culture 65 million years
  beyond its extinction.
- Thus `rawr` (or "**R**idiculously **A**ccumulating **W**orks to **R**ead" for long).

#### Why HTML only?
HTML compresses down the most.
AO3 also generates the ebook and PDF versions from the HTML.
HTML is king.

## Disclaimer
This CLI is to manage HTML files that **you** download _manually_. I do not
support bots or other automated tools that auto-download fics, or put other
unnecessary strain on the service. AO3 had to implement WAF and rate-limiting
for a reason: don't be a dick about it.

## Installation
Build from source. If you don't know how, ask the nerdiest friend you've got.

```shell
cargo build --release
```

> If on Linux, use `cargo deploy` alias to enable super muscle builds 💪

## Quick Start

1. `rawr init` to create a configuration file
2. Download some fics from AO3 in HTML format
3. `rawr import "Downloads/"`
4. Go outside for a walk, you haven't left the apartment in days.

### License
This project will remain proprietary, yet source-available, until a version
`1.0` release (read: I haven't decided which open-source license I want to use yet)
