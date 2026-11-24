//! Mishearings the slides can fix. Speech recognition mangles names and
//! terms ("Grogbot" for "Grok Bot", "Century" for "Sentry") that the slide on
//! screen spells out. Candidates are found here, without a model: a narration
//! word (or two joined) that is close in spelling or sound to a term on the
//! slide being shown or its neighbours, where the slide's term never appears
//! in the transcript as written. Close ones are applied; the rest are listed.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::deck::Deck;

/// A possible mishearing.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Candidate {
    /// As the transcription has it.
    pub heard: String,
    /// As the slides spell it.
    pub correct: String,
    /// Parts whose narration has the heard form, by number.
    pub parts: Vec<usize>,
}

/// Common English words are never treated as mishearings.
const COMMON: &str = "about above after again against also always among another answer anything around asked \
    away back became because become been before being below best better between both bring build built \
    called came cannot case change come could course days does doing done down during each early either \
    else enough even ever every example fact feel find first five following form four from gave general \
    give given goes going good great group hand hard have having head help here high himself home house \
    however idea important into itself just keep kind knew know known large last later least left less \
    life like line little long look made make making many maybe mean might more most move much must name \
    near need never next nice night nothing number often once only open order other others over part \
    people place point possible power pretty problem public put quite rather real really right said same \
    saying says second seem seen sense several shall should show side since small some something sort \
    start state still such sure system take taken tell than that their them then there these they thing \
    things think this those though thought three through time times today together told took toward \
    turn under until upon used using very want wanted ways well went were what when where whether which \
    while whole will with within without word words work world would write year years your yourself \
    these their there here hear where were wear what that than then when well will with were \
    reply share send save edit cancel close delete copy paste search settings page home back next done \
    open view file files help menu more less login logout sign submit";

fn key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn strip(w: &str) -> &str {
    let w = w.trim_matches(|c: char| !c.is_alphanumeric());
    // A possessive is the same name: "Grockbot's".
    w.strip_suffix("'s").or_else(|| w.strip_suffix("’s")).unwrap_or(w)
}

/// A rough sound key: first letter kept, soft c to s, ph to f, k/q/ck to k,
/// z to s, then vowels, h, w and y dropped and doubles collapsed.
fn sound(k: &str) -> String {
    let s = k.replace("ph", "f").replace("ck", "k");
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        let c = match c {
            'c' if matches!(next, Some('e' | 'i' | 'y')) => 's',
            'c' | 'q' => 'k',
            'z' => 's',
            _ => c,
        };
        if i > 0 && matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'h' | 'w' | 'y') {
            continue;
        }
        if out.ends_with(c) {
            continue;
        }
        out.push(c);
    }
    out
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + usize::from(ca != cb));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn close(heard: &str, term: &str) -> bool {
    if heard == term || heard.starts_with(term) || term.starts_with(heard) {
        return false;
    }
    let n = heard.chars().count().max(term.chars().count());
    let limit = if n <= 5 {
        1
    } else if n <= 9 {
        2
    } else {
        3
    };
    let d = distance(heard, term);
    let same_start = heard.chars().next() == term.chars().next();
    (d <= limit && same_start) || d <= 1 || (sound(heard) == sound(term) && d <= limit + 1)
}

/// A slide term worth correcting towards: a name or a piece of jargon, which
/// speech recognition cannot know how to spell. Ordinary words are not.
#[derive(Clone, Debug)]
struct Term {
    key: String,
    text: String,
    /// Two words, matched against one heard word ("Grok Bot" heard "Grogbot").
    pair: bool,
}

fn name_like(
    raw: &str,
    word: &str,
    position: usize,
    line_len: usize,
    common: &HashSet<&str>,
) -> bool {
    let k = key(word);
    if common.contains(k.as_str()) {
        return false;
    }
    let codeish = raw.contains(['@', '_', '/']) || word.chars().any(|c| c.is_ascii_digit());
    // Capitalised mid-line, or alone on its line: a name, not a sentence start.
    let capitalised =
        word.chars().next().is_some_and(char::is_uppercase) && (position > 0 || line_len == 1);
    // Lowercase code names tend to open with consonants: pstack, npm, xcrun.
    let consonants = k.chars().take(3).filter(|c| !"aeiouy".contains(*c)).count() == 3;
    codeish || capitalised || consonants
}

/// Terms on a slide: name-like single words of 4+ characters, and two-word
/// names whose words are both capitalised.
fn terms(lines: &[&str], common: &HashSet<&str>) -> Vec<Term> {
    let mut out = Vec::new();
    for line in lines {
        let raw: Vec<&str> = line.split_whitespace().collect();
        let ws: Vec<(&str, &str)> = raw
            .iter()
            .map(|r| (*r, strip(r)))
            .filter(|(_, w)| !w.is_empty())
            .collect();
        for (i, &(r, w)) in ws.iter().enumerate() {
            if name_like(r, w, i, ws.len(), common) {
                out.push(Term {
                    key: key(w),
                    text: w.to_string(),
                    pair: false,
                });
            }
            if let Some(&(_, n)) = ws.get(i + 1) {
                let caps = |s: &str| s.chars().next().is_some_and(char::is_uppercase);
                if caps(w) && caps(n) {
                    out.push(Term {
                        key: key(w) + &key(n),
                        text: format!("{w} {n}"),
                        pair: true,
                    });
                }
            }
        }
    }
    out.retain(|t| t.key.chars().count() >= 4 && t.key.chars().any(char::is_alphabetic));
    out
}

/// Words that share a long stem are forms of one word, not mishearings:
/// engineering and engineers, imported and imports.
fn same_stem(a: &str, b: &str) -> bool {
    let common = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    common >= 5 || (common >= 4 && common + 3 >= a.chars().count().min(b.chars().count()))
}

/// How sure a candidate is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    /// One letter apart, or the same sound: applied to the narration.
    Strong,
    /// Further apart: listed only.
    Weak,
}

/// Candidates, in order of the parts they occur in. A target is a name on
/// any slide that the recognizer never wrote as the slide does: names and
/// jargon it cannot spell are exactly the ones missing from the transcript,
/// while ordinary words it gets right somewhere.
pub fn candidates(deck: &Deck) -> Vec<(Candidate, Strength)> {
    let common: HashSet<&str> = COMMON.split_whitespace().collect();
    let lines_of = |k: usize| -> Vec<&str> {
        deck.parts
            .get(k)
            .and_then(|p| p.slide_text.as_ref())
            .map(|t| t.title.iter().chain(&t.text).map(String::as_str).collect())
            .unwrap_or_default()
    };
    // Every word shown on a slide: a heard word a slide shows is no mishearing.
    let shown: HashSet<String> = (0..deck.parts.len())
        .flat_map(|k| {
            lines_of(k)
                .into_iter()
                .flat_map(|l| l.split_whitespace().map(key))
                .collect::<Vec<_>>()
        })
        .collect();
    // What the recognizer wrote, and how often: a word it wrote 4 or more times
    // is one it knows.
    let said: Vec<String> = deck
        .words
        .iter()
        .map(|w| key(&w.text))
        .filter(|k| !k.is_empty())
        .collect();
    let mut written: HashMap<&str, usize> = HashMap::new();
    for k in &said {
        *written.entry(k.as_str()).or_default() += 1;
    }
    // A name the recognizer wrote correctly 4 or more times is one it knows.
    let pairs: Vec<String> = said.windows(2).map(|w| w[0].clone() + &w[1]).collect();
    let spelled = |t: &Term| {
        let n = if t.pair {
            pairs.iter().filter(|p| **p == t.key).count()
        } else {
            written.get(t.key.as_str()).copied().unwrap_or(0)
        };
        n >= 4
    };
    let mut targets: Vec<Term> = (0..deck.parts.len())
        .flat_map(|k| terms(&lines_of(k), &common))
        .collect();
    targets.retain(|t| !spelled(t));
    targets.sort_by(|a, b| a.key.cmp(&b.key).then(b.pair.cmp(&a.pair)));
    targets.dedup_by(|a, b| a.key == b.key);

    let mut found: HashMap<(String, String), (Vec<usize>, Strength)> = HashMap::new();
    for p in &deck.parts {
        for w in p
            .narration
            .split_whitespace()
            .map(strip)
            .filter(|w| !w.is_empty())
        {
            let ku = key(w);
            if ku.chars().count() < 4 || shown.contains(&ku) || common.contains(ku.as_str()) {
                continue;
            }
            let frequent = written.get(ku.as_str()).copied().unwrap_or(0) >= 4;
            for t in &targets {
                // A two-word name written as one word is the same name.
                let spacing = t.pair && ku == t.key;
                if !spacing && (frequent || !close(&ku, &t.key) || same_stem(&ku, &t.key)) {
                    continue;
                }
                let strong = spacing || distance(&ku, &t.key) <= 1 || sound(&ku) == sound(&t.key);
                let strength = if strong {
                    Strength::Strong
                } else {
                    Strength::Weak
                };
                let e = found
                    .entry((w.to_string(), t.text.clone()))
                    .or_insert((Vec::new(), strength));
                if !e.0.contains(&p.number) {
                    e.0.push(p.number);
                }
                break;
            }
        }
    }
    let mut out: Vec<(Candidate, Strength)> = found
        .into_iter()
        .map(|((heard, correct), (parts, strength))| {
            (
                Candidate {
                    heard,
                    correct,
                    parts,
                },
                strength,
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.parts.cmp(&b.0.parts).then(a.0.heard.cmp(&b.0.heard)));
    out
}

/// Replaces whole-word occurrences of each heard form in the narration.
pub fn apply(deck: &mut Deck, confirmed: &[Candidate]) {
    for p in &mut deck.parts {
        for c in confirmed {
            if !c.parts.contains(&p.number) {
                continue;
            }
            let (h, n) = (p.narration.to_lowercase(), c.heard.to_lowercase());
            if h.len() != p.narration.len() {
                continue;
            }
            let mut spots = Vec::new();
            let mut from = 0;
            while let Some(i) = h[from..].find(&n) {
                let (a, b) = (from + i, from + i + n.len());
                let before = h[..a]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric());
                let after = h[b..].chars().next().is_none_or(|c| !c.is_alphanumeric());
                if before && after {
                    spots.push((a, b));
                }
                from = b;
            }
            for (a, b) in spots.into_iter().rev() {
                p.narration.replace_range(a..b, &c.correct);
            }
        }
    }
}
