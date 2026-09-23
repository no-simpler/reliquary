//! Whether a domain still resolves, routes and authenticates the way the repo
//! says it does — and how long it has left to do it.
//!
//! One subject, one station. Registration and mail authentication answer the
//! same question, *is this domain still the one the repo describes*, and they
//! fail the same way: **silently**. A DNS record is the only security control in
//! this estate that lives entirely off the machine, so nothing here rots into a
//! broken build or a failed command. SPF loosens, a DKIM selector stops
//! resolving, DNSSEC lapses at the registrar, a renewal date passes — and the
//! first symptom is mail that quietly stops authenticating, or a domain in an
//! auction.
//!
//! **The expiry alarm must not travel through the domain.** Every account that
//! signs in with an address on it inherits its expiry, and on expiry the
//! nameservers move to parking, so notices addressed to the domain are lost
//! exactly when they matter. Reading the registry directly is what keeps the
//! alarm out of the failing channel.
//!
//! **`dig` is the oracle, not a resolver written here.** The house rule prefers
//! a machine-readable interface, and `dig +short` is the one DNS offers: one
//! record per line, no prose, no locale. A second resolver implementation is how
//! a checker comes to disagree with the thing it checks. The parse contract is
//! narrow and stated where it is applied — `TXT` arrives as quoted segments to
//! be concatenated, `MX` as a preference and a host.
//!
//! **The repo asserts; the station reports drift.** Every expected value is
//! configuration rather than knowledge baked in here: a station that knew one
//! mail provider's hostnames would be wrong the day the provider changed them,
//! and would have to be edited rather than re-measured. What is compared is the
//! string the repo committed to against the string the world returns.
//!
//! Costs the network, so it runs only under `--deep`.

use std::time::Duration;

use anyhow::{Context as _, Result};
use serde::Deserialize;

use relic_core::finding::{Detail, Finding, FixHint, Outcome, StationId, Summary};
use relic_core::tool::Tool;

use crate::station::{Context, Station};

/// What the repo asserts about its domains, `$HOME`-relative.
const CONFIG: &str = ".config/assay/domains.toml";

/// The resolver's own command-line interface.
const DIG: &str = "dig";

/// How the registry is read. A bedrock member, so its absence is a fault of the
/// machine rather than a reason to skip.
const CURL: &str = "curl";

/// How long a lookup has to answer. Generous: a cold resolver and a registry on
/// another continent are both in scope.
const BUDGET: Duration = Duration::from_secs(20);

/// Below this many days to expiry, say so.
const EXPIRY_WARN_DAYS: i64 = 180;

/// Below this many days, it is no longer a warning.
const EXPIRY_FAIL_DAYS: i64 = 60;

/// The bootstrap that maps a domain to its registry's RDAP service.
const RDAP: &str = "https://rdap.org/domain/";

/// Seconds in a day, for turning two instants into a countdown.
const DAY: i64 = 86_400;

/// What the repo asserts, as read from the config file.
#[derive(Debug, Default, Deserialize)]
struct Domains {
    /// One entry per domain. Absent means there is nothing to check.
    #[serde(default)]
    domain: Vec<Expected>,
}

/// One domain, and what it should still be saying.
///
/// Every field but the name is optional, because a domain that sends no mail
/// still has a registration worth watching and should not have to declare empty
/// mail policy to get it.
#[derive(Debug, Default, Deserialize)]
struct Expected {
    /// The domain itself.
    name: String,
    /// The mail exchangers, by host, in any order.
    #[serde(default)]
    mx: Vec<String>,
    /// The whole SPF record, compared verbatim.
    spf: Option<String>,
    /// The whole DMARC record, compared verbatim.
    dmarc: Option<String>,
    /// The DKIM selectors that must still resolve.
    #[serde(default)]
    dkim: Vec<String>,
    /// Whether the parent zone should carry a delegation signer.
    #[serde(default)]
    dnssec: bool,
    /// Whether the registry should still refuse a transfer.
    #[serde(default)]
    transfer_lock: bool,
}

/// What the world actually returned.
///
/// Separated from the judging so that every rule below is testable without a
/// resolver, a registry or a network.
#[derive(Debug, Default)]
struct Observed {
    /// Mail exchanger hosts, normalised.
    mx: Vec<String>,
    /// Every `v=spf1` record found at the apex, verbatim.
    spf: Vec<String>,
    /// Every record found at `_dmarc`, verbatim.
    dmarc: Vec<String>,
    /// Each selector asked for, and whether it resolved.
    dkim: Vec<(String, bool)>,
    /// Whether the parent zone carries a delegation signer.
    dnssec: bool,
    /// What the registry said, when it could be reached.
    registration: Option<Registration>,
}

/// The registry's answer about one domain.
#[derive(Debug, Default)]
struct Registration {
    /// Days until expiry, negative once past.
    days_left: Option<i64>,
    /// Whether any status says a transfer is prohibited.
    transfer_locked: bool,
}

/// The station.
pub struct DomainPosture {
    /// Its name on the command line.
    id: StationId,
}

impl Default for DomainPosture {
    fn default() -> Self {
        Self {
            id: StationId::from_static("domain"),
        }
    }
}

impl Station for DomainPosture {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "every domain still resolves, routes and authenticates as the repo says, and is not expiring"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        if !cx.deep() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "reading DNS and the registry costs the network; run with --deep",
            )));
        }

        let file = cx.at(CONFIG);
        if !file.exists() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "no domain is declared, so there is nothing to hold the world to",
            )));
        }
        let text = fs_err::read_to_string(&file)
            .with_context(|| format!("the domain file at {file} could not be read"))?;
        let declared: Domains = toml::from_str(&text)
            .with_context(|| format!("{file} is not a domain file this version understands"))?;

        if declared.domain.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "the domain file declares no domain",
            )));
        }

        let Some(dig) = Tool::find(DIG) else {
            return Ok(Outcome::Skipped(Summary::lossy(
                "dig is not on this machine, so nothing can be asked of DNS",
            )));
        };
        let curl = Tool::find(CURL);

        let mut findings = Vec::new();
        for expected in &declared.domain {
            let observed = look_up(&dig, curl.as_ref(), expected);
            if curl.is_none() {
                findings.push(self.id.note(Summary::lossy(
                    "curl is absent, so no registration could be read — and curl is bedrock",
                )));
            }
            findings.extend(self.judge(expected, &observed));
        }
        Ok(Outcome::Ran(findings))
    }
}

impl DomainPosture {
    /// Hold one domain to what the repo said about it.
    fn judge(&self, want: &Expected, got: &Observed) -> Vec<Finding> {
        let mut findings = Vec::new();
        let at = &want.name;

        findings.extend(self.judge_mx(at, want, got));
        findings.extend(self.judge_spf(at, want, got));
        findings.extend(self.judge_dmarc(at, want, got));
        findings.extend(self.judge_dkim(at, got));
        findings.extend(self.judge_registration(at, want, got));

        if want.dnssec && !got.dnssec {
            findings.push(
                self.id
                    .broken(Summary::lossy(&format!(
                        "{at} publishes no DS record, so the zone is no longer signed"
                    )))
                    .detailed_with(Detail::new(
                        "DNSSEC is what makes every other answer here trustworthy, and it is what \
                         lets a sending server validate DANE on the mail exchanger. Losing it \
                         silently downgrades inbound mail protection without changing any record \
                         a person would look at.",
                    ))
                    .fixed_by(FixHint::lossy("re-enable DNSSEC at the registrar")),
            );
        }

        findings
    }

    /// Mail still goes where the repo says.
    fn judge_mx(&self, at: &str, want: &Expected, got: &Observed) -> Vec<Finding> {
        if want.mx.is_empty() {
            return Vec::new();
        }
        let expected = normalised(&want.mx);
        if expected == normalised(&got.mx) {
            return Vec::new();
        }
        vec![
            self.id
                .broken(Summary::lossy(&format!(
                    "{at} no longer routes mail to the exchangers the repo declares"
                )))
                .detailed_with(Detail::new(
                    "Inbound mail for every address on this domain follows these records. A \
                     change nobody made here is a redirection; a change somebody made is a repo \
                     that has stopped describing the machine.",
                ))
                .fixed_by(FixHint::lossy(
                    "restore the MX records, or update the declaration if the change was intended",
                )),
        ]
    }

    /// Exactly one sender policy, and the one that was committed to.
    fn judge_spf(&self, at: &str, want: &Expected, got: &Observed) -> Vec<Finding> {
        let Some(expected) = want.spf.as_deref() else {
            return Vec::new();
        };
        if got.spf.len() > 1 {
            return vec![
                self.id
                    .broken(Summary::lossy(&format!(
                        "{at} publishes {} SPF records, and more than one is none",
                        got.spf.len()
                    )))
                    .detailed_with(Detail::new(
                        "A receiver that finds two SPF records is required to treat the result as \
                         permanently erroneous, which fails the check rather than passing it \
                         twice.",
                    ))
                    .fixed_by(FixHint::lossy("merge them into a single TXT record")),
            ];
        }
        match got.spf.first() {
            Some(actual) if actual == expected => Vec::new(),
            Some(_) => vec![
                self.id
                    .broken(Summary::lossy(&format!(
                        "{at} publishes an SPF record the repo does not declare"
                    )))
                    .detailed_with(Detail::new(
                        "The value is compared verbatim, so a loosened qualifier and an added \
                         sender read the same here: something changed that nothing on this \
                         machine recorded.",
                    ))
                    .fixed_by(FixHint::lossy(
                        "restore the declared record, or update the declaration",
                    )),
            ],
            None => vec![
                self.id
                    .broken(Summary::lossy(&format!("{at} publishes no SPF record")))
                    .fixed_by(FixHint::lossy("restore the declared SPF record")),
            ],
        }
    }

    /// The policy a receiver applies when authentication fails.
    fn judge_dmarc(&self, at: &str, want: &Expected, got: &Observed) -> Vec<Finding> {
        let Some(expected) = want.dmarc.as_deref() else {
            return Vec::new();
        };
        if got.dmarc.iter().any(|actual| actual == expected) {
            return Vec::new();
        }
        let summary = if got.dmarc.is_empty() {
            format!("{at} publishes no DMARC record")
        } else {
            format!("{at} publishes a DMARC record the repo does not declare")
        };
        vec![
            self.id
                .broken(Summary::lossy(&summary))
                .detailed_with(Detail::new(
                    "DMARC is the only one of the three that tells a receiver what to *do*. \
                     Without it, SPF and DKIM are measurements nobody acts on, and the domain can \
                     be spoofed at no cost.",
                ))
                .fixed_by(FixHint::lossy(
                    "restore the declared _dmarc record, or update the declaration",
                )),
        ]
    }

    /// Every selector still resolves, so outgoing mail is still signed.
    fn judge_dkim(&self, at: &str, got: &Observed) -> Vec<Finding> {
        got.dkim
            .iter()
            .filter(|(_, resolved)| !resolved)
            .map(|(selector, _)| {
                self.id
                    .broken(Summary::lossy(&format!(
                        "{at} selector {selector} does not resolve, so mail signed with it cannot verify"
                    )))
                    .detailed_with(Detail::new(
                        "A DKIM failure is worse than an absent signature once DMARC is \
                         enforcing: the domain's own outgoing mail starts being refused, and the \
                         bounce arrives at the domain that is broken.",
                    ))
                    .fixed_by(FixHint::lossy("restore the selector's CNAME at the DNS host"))
            })
            .collect()
    }

    /// The registration itself: how long it has, and whether it can be taken.
    fn judge_registration(&self, at: &str, want: &Expected, got: &Observed) -> Vec<Finding> {
        let Some(registration) = got.registration.as_ref() else {
            return vec![self.id.note(Summary::lossy(&format!(
                "the registry could not be read for {at}, so its expiry is unknown"
            )))];
        };

        let mut findings = Vec::new();
        match registration.days_left {
            Some(days) if days < EXPIRY_FAIL_DAYS => findings.push(
                self.id
                    .broken(Summary::lossy(&format!("{at} expires in {days} days")))
                    .detailed_with(Detail::new(
                        "On expiry the nameservers move to parking, so mail to addresses on this \
                         domain stops — including the notices warning that it has expired. Every \
                         account that signs in with such an address inherits this date.",
                    ))
                    .fixed_by(FixHint::lossy("renew it at the registrar")),
            ),
            Some(days) if days < EXPIRY_WARN_DAYS => findings.push(
                self.id
                    .soft(Summary::lossy(&format!("{at} expires in {days} days")))
                    .fixed_by(FixHint::lossy("renew it at the registrar")),
            ),
            Some(_) => {}
            None => findings.push(self.id.note(Summary::lossy(&format!(
                "the registry reported no expiry date for {at}"
            )))),
        }

        if want.transfer_lock && !registration.transfer_locked {
            findings.push(
                self.id
                    .broken(Summary::lossy(&format!(
                        "{at} is not transfer-locked at the registry"
                    )))
                    .detailed_with(Detail::new(
                        "The lock is what makes a stolen registrar session recoverable: without \
                         it, a transfer completes and the domain leaves with its mail.",
                    ))
                    .fixed_by(FixHint::lossy("re-enable the registrar lock")),
            );
        }

        findings
    }
}

/// Ask DNS and the registry everything one domain's declaration wants to know.
fn look_up(dig: &Tool, curl: Option<&Tool>, want: &Expected) -> Observed {
    let apex = want.name.as_str();
    Observed {
        mx: short(dig, "MX", apex)
            .iter()
            .filter_map(|line| mx_host(line))
            .collect(),
        spf: short(dig, "TXT", apex)
            .iter()
            .map(|line| unquote(line))
            .filter(|record| record.starts_with("v=spf1"))
            .collect(),
        dmarc: short(dig, "TXT", &format!("_dmarc.{apex}"))
            .iter()
            .map(|line| unquote(line))
            .filter(|record| record.starts_with("v=DMARC1"))
            .collect(),
        dkim: want
            .dkim
            .iter()
            .map(|selector| {
                let at = format!("{selector}._domainkey.{apex}");
                let resolved =
                    !short(dig, "CNAME", &at).is_empty() || !short(dig, "TXT", &at).is_empty();
                (selector.clone(), resolved)
            })
            .collect(),
        dnssec: !short(dig, "DS", apex).is_empty(),
        registration: curl.and_then(|curl| registration(curl, apex)),
    }
}

/// One `dig +short` answer per line, blanks dropped.
///
/// A failure to resolve and an empty answer are the same thing here: the record
/// is not there. `dig` exits zero for both, and the distinction between them is
/// not one any rule above acts on.
fn short(dig: &Tool, kind: &str, name: &str) -> Vec<String> {
    let mut command = dig.command();
    command.args(["+short", "+time=5", "+tries=2", kind, name]);
    dig.capture_within(&mut command, BUDGET)
        .map(|output| {
            output
                .stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The host out of a `preference host.` answer.
fn mx_host(line: &str) -> Option<String> {
    line.split_whitespace().nth(1).map(normalise)
}

/// A TXT record out of the quoted segments `dig` prints it as.
///
/// A record longer than 255 bytes arrives as several quoted strings on one
/// line, and concatenating them is what the protocol says they mean. A line
/// with no quotes at all is taken verbatim, so a resolver that stops quoting
/// does not silently yield the empty string.
fn unquote(line: &str) -> String {
    if !line.contains('"') {
        return line.trim().to_owned();
    }
    let mut out = String::new();
    let mut inside = false;
    for part in line.split('"') {
        if inside {
            out.push_str(part);
        }
        inside = !inside;
    }
    out
}

/// Compare hostnames without caring about the root dot or case.
fn normalise(host: &str) -> String {
    host.trim().trim_end_matches('.').to_lowercase()
}

/// The same, for a set whose order carries nothing.
fn normalised(hosts: &[String]) -> Vec<String> {
    let mut all: Vec<String> = hosts.iter().map(|host| normalise(host)).collect();
    all.sort();
    all.dedup();
    all
}

/// What the registry says, or nothing when it could not be asked.
fn registration(curl: &Tool, domain: &str) -> Option<Registration> {
    let mut command = curl.command();
    command.args([
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        "15",
        "--header",
        "Accept: application/rdap+json",
        &format!("{RDAP}{domain}"),
    ]);
    let output = curl.capture_within(&mut command, BUDGET).ok()?;
    let body: serde_json::Value = serde_json::from_str(&output.stdout).ok()?;
    Some(read_registration(&body))
}

/// Read the two fields this station acts on out of an RDAP response.
///
/// Everything else the registry returns is ignored on purpose: a station that
/// transcribed the whole object would have to be edited whenever a registry
/// added a field.
fn read_registration(body: &serde_json::Value) -> Registration {
    let days_left = body
        .get("events")
        .and_then(serde_json::Value::as_array)
        .and_then(|events| {
            events.iter().find(|event| {
                event.get("eventAction").and_then(serde_json::Value::as_str) == Some("expiration")
            })
        })
        .and_then(|event| event.get("eventDate"))
        .and_then(serde_json::Value::as_str)
        .and_then(days_until);

    let transfer_locked = body
        .get("status")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|status| {
            status
                .iter()
                .filter_map(serde_json::Value::as_str)
                .any(|state| state.to_lowercase().contains("transfer prohibited"))
        });

    Registration {
        days_left,
        transfer_locked,
    }
}

/// Whole days from now until an RDAP timestamp, negative once past.
fn days_until(stamp: &str) -> Option<i64> {
    let expiry: jiff::Timestamp = stamp.parse().ok()?;
    Some((expiry.as_second() - jiff::Timestamp::now().as_second()).div_euclid(DAY))
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;

    fn station() -> DomainPosture {
        DomainPosture::default()
    }

    fn declared() -> Expected {
        Expected {
            name: "example.test".to_owned(),
            mx: vec![
                "mail.provider.test.".to_owned(),
                "alt.provider.test.".to_owned(),
            ],
            spf: Some("v=spf1 include:_spf.provider.test -all".to_owned()),
            dmarc: Some("v=DMARC1; p=reject".to_owned()),
            dkim: vec!["one".to_owned(), "two".to_owned()],
            dnssec: true,
            transfer_lock: true,
        }
    }

    fn healthy() -> Observed {
        Observed {
            mx: vec![
                "alt.provider.test".to_owned(),
                "mail.provider.test".to_owned(),
            ],
            spf: vec!["v=spf1 include:_spf.provider.test -all".to_owned()],
            dmarc: vec!["v=DMARC1; p=reject".to_owned()],
            dkim: vec![("one".to_owned(), true), ("two".to_owned(), true)],
            dnssec: true,
            registration: Some(Registration {
                days_left: Some(400),
                transfer_locked: true,
            }),
        }
    }

    fn severities(findings: &[Finding]) -> Vec<Severity> {
        findings.iter().map(|finding| finding.severity).collect()
    }

    #[test]
    fn a_domain_that_matches_its_declaration_has_nothing_to_say() {
        assert!(station().judge(&declared(), &healthy()).is_empty());
    }

    #[test]
    fn mx_order_is_not_part_of_the_answer() {
        let mut got = healthy();
        got.mx.reverse();
        assert!(station().judge(&declared(), &got).is_empty());
    }

    #[test]
    fn a_trailing_root_dot_is_not_a_difference() {
        let mut got = healthy();
        got.mx = vec![
            "MAIL.provider.test.".to_owned(),
            "alt.provider.test".to_owned(),
        ];
        assert!(station().judge(&declared(), &got).is_empty());
    }

    #[test]
    fn two_sender_policies_are_none() {
        let mut got = healthy();
        got.spf
            .push("v=spf1 include:elsewhere.test ~all".to_owned());
        let findings = station().judge(&declared(), &got);
        assert_eq!(severities(&findings), vec![Severity::Broken]);
        assert!(findings[0].summary.as_str().contains("2 SPF records"));
    }

    #[test]
    fn a_loosened_qualifier_is_drift_like_any_other() {
        let mut got = healthy();
        got.spf = vec!["v=spf1 include:_spf.provider.test ~all".to_owned()];
        assert_eq!(
            severities(&station().judge(&declared(), &got)),
            vec![Severity::Broken]
        );
    }

    #[test]
    fn a_selector_that_stopped_resolving_is_named() {
        let mut got = healthy();
        got.dkim = vec![("one".to_owned(), true), ("two".to_owned(), false)];
        let findings = station().judge(&declared(), &got);
        assert_eq!(severities(&findings), vec![Severity::Broken]);
        assert!(findings[0].summary.as_str().contains("two"));
    }

    #[test]
    fn an_unsigned_zone_is_broken_when_the_repo_says_it_is_signed() {
        let mut got = healthy();
        got.dnssec = false;
        let findings = station().judge(&declared(), &got);
        assert_eq!(severities(&findings), vec![Severity::Broken]);
        assert!(findings[0].summary.as_str().contains("DS record"));
    }

    #[test]
    fn a_domain_that_declares_no_mail_policy_is_only_a_registration() {
        let want = Expected {
            name: "example.test".to_owned(),
            transfer_lock: true,
            ..Expected::default()
        };
        let got = Observed {
            registration: Some(Registration {
                days_left: Some(400),
                transfer_locked: true,
            }),
            ..Observed::default()
        };
        assert!(station().judge(&want, &got).is_empty());
    }

    #[test]
    fn expiry_warns_before_it_fails() {
        for (days, expected) in [
            (EXPIRY_WARN_DAYS, None),
            (EXPIRY_WARN_DAYS - 1, Some(Severity::Soft)),
            (EXPIRY_FAIL_DAYS, Some(Severity::Soft)),
            (EXPIRY_FAIL_DAYS - 1, Some(Severity::Broken)),
            (-1, Some(Severity::Broken)),
        ] {
            let mut got = healthy();
            got.registration = Some(Registration {
                days_left: Some(days),
                transfer_locked: true,
            });
            let findings = station().judge(&declared(), &got);
            assert_eq!(
                severities(&findings),
                expected.into_iter().collect::<Vec<_>>(),
                "at {days} days"
            );
        }
    }

    #[test]
    fn an_unlocked_registration_can_be_transferred_away() {
        let mut got = healthy();
        got.registration = Some(Registration {
            days_left: Some(400),
            transfer_locked: false,
        });
        let findings = station().judge(&declared(), &got);
        assert_eq!(severities(&findings), vec![Severity::Broken]);
        assert!(findings[0].summary.as_str().contains("transfer-locked"));
    }

    #[test]
    fn a_registry_that_could_not_be_read_is_a_note_and_never_a_verdict() {
        let mut got = healthy();
        got.registration = None;
        let findings = station().judge(&declared(), &got);
        assert_eq!(severities(&findings), vec![Severity::Note]);
    }

    #[test]
    fn a_long_record_is_the_concatenation_of_its_segments() {
        assert_eq!(
            unquote(r#""v=spf1 " "include:x -all""#),
            "v=spf1 include:x -all"
        );
        assert_eq!(unquote(r#""one""#), "one");
        assert_eq!(unquote("unquoted"), "unquoted");
    }

    #[test]
    fn an_mx_answer_is_a_preference_and_a_host() {
        assert_eq!(
            mx_host("10 mail.provider.test."),
            Some("mail.provider.test".to_owned())
        );
        assert_eq!(mx_host("garbage"), None);
    }

    #[test]
    fn rdap_is_read_for_two_fields_and_no_others() {
        let body = serde_json::json!({
            "objectClassName": "domain",
            "status": ["client transfer prohibited", "client delete prohibited"],
            "events": [
                {"eventAction": "registration", "eventDate": "2021-06-09T09:42:01Z"},
                {"eventAction": "expiration", "eventDate": "2999-01-01T00:00:00Z"}
            ]
        });
        let read = read_registration(&body);
        assert!(read.transfer_locked);
        assert!(read.days_left.is_some_and(|days| days > 100_000));
    }

    #[test]
    fn a_registry_that_answers_without_an_expiry_says_so_rather_than_guessing() {
        let body = serde_json::json!({ "status": [], "events": [] });
        let read = read_registration(&body);
        assert!(!read.transfer_locked);
        assert_eq!(read.days_left, None);
    }
}
