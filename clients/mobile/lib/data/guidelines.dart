// Guidelines (AUR-GL-01) without widgets: the limits of B4 and how close a
// text or a block is to them, the groups of the link list (reports_to, B2),
// the block a run would get with a change (P-56), and the fixed texts.

import 'package:shared_preferences/shared_preferences.dart';

import '../core/net/connection.dart' show NotSentException;
import '../core/net/protocol.dart';
import 'protocol/protocol.dart';
import 'views.dart';

/// B4: the characters of one guideline's text.
const guidelineMaxChars = 8000;

/// B4: the characters of guidelines one run gets together.
const blockMaxChars = 16000;

/// From this share of a limit on, the counter turns orange ("Close to the limit", GL-07 AC2).
const warnShare = 0.8;

/// How a count stands against its limit.
enum LimitLevel { ok, close, over }

LimitLevel limitLevel(int chars, int max) => chars > max ? LimitLevel.over : chars >= max * warnShare ? LimitLevel.close : LimitLevel.ok;

/// "6 120": thousands apart with a no-break space, as the design writes them.
String chars(int n) {
  final digits = n.abs().toString();
  final out = StringBuffer(n < 0 ? '-' : '');
  for (var i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 == 0) out.write(' ');
    out.write(digits[i]);
  }
  return out.toString();
}

/// The characters the server counts: Unicode code points, like Rust's `chars().count()`.
int charCount(String text) => text.runes.length;

/// "6 120 / 8 000".
String counterLine(int n, {int max = guidelineMaxChars}) => '${chars(n)} / ${chars(max)}';

/// The words that go with the bar, so its colour never stands alone (ui §9).
String? limitNote(int n, {int max = guidelineMaxChars}) => switch (limitLevel(n, max)) {
      LimitLevel.ok => null,
      LimitLevel.close => 'Close to the limit',
      LimitLevel.over => '${chars(n - max)} characters over the limit of ${chars(max)}',
    };

/// How a screen reader reads the counter.
String counterLabel(int n, {int max = guidelineMaxChars}) => '${chars(n)} of ${chars(max)} characters';

/// One context a run can have (B2): an employee alone, an employee in a project, or a project alone.
typedef BlockContext = ({String? employeeId, String? projectId});

/// The contexts a link to [target] reaches, as the server checks them
/// (`combinations_over`): an employee alone and in each project; a project
/// alone and with each employee. Open work does not matter, paused ones count,
/// archived ones do not (B4).
List<BlockContext> contextsOf(GuidelineTarget target, List<EmployeeSummary> employees, List<ProjectSummary> projects) {
  final projectIds = [for (final p in projects) if (p.status != ProjectStatus.archived) p.id];
  final employeeIds = [for (final e in employees) if (e.status != LifecycleStatus.archived) e.id];
  return switch (target) {
    GuidelineTargetEmployee(:final id) => [
        if (employeeIds.contains(id)) ...[
          (employeeId: id, projectId: null),
          for (final p in projectIds) (employeeId: id, projectId: p),
        ],
      ],
    GuidelineTargetProject(:final id) => [
        if (projectIds.contains(id)) ...[
          (employeeId: null, projectId: id),
          for (final e in employeeIds) (employeeId: e, projectId: id),
        ],
      ],
  };
}

/// Every context [links] reach, once each.
List<BlockContext> contextsOfAll(Iterable<GuidelineTarget> links, List<EmployeeSummary> employees, List<ProjectSummary> projects) => {
      for (final l in links) ...contextsOf(l, employees, projects),
    }.toList();

/// What an employee's runs look like per project it works in (GL-07): alone,
/// then each project with open work. For showing, not for the limit.
List<BlockContext> workContextsOf(EmployeeSummary? employee, String id) => [
      (employeeId: id, projectId: null),
      for (final p in employee?.projectsActive ?? const <String>[]) (employeeId: id, projectId: p),
    ];

/// The block of [set] with guideline [guidelineId] at [withChars] characters
/// (null: without it). A guideline already in the set counts once (B3, P-55).
int blockWith(EffectiveGuidelines set, String guidelineId, int? withChars) {
  final own = set.guidelines.where((g) => g.id == guidelineId).firstOrNull;
  final rest = set.totalChars - (own?.chars ?? 0);
  return rest + (withChars ?? 0);
}

/// The largest block over contexts: its size and where.
typedef LargestBlock = ({int chars, BlockContext context});

LargestBlock? largestOf(Iterable<(BlockContext, int)> sizes) {
  LargestBlock? best;
  for (final (ctx, n) in sizes) {
    if (best == null || n > best.chars) best = (chars: n, context: ctx);
  }
  return best;
}

/// "Bridge in aurelius", "Bridge", "everyone in aurelius".
String contextName(BlockContext ctx, {required String Function(String) employee, required String Function(String) project}) => switch (ctx) {
      (employeeId: final e?, projectId: final p?) => '${employee(e)} in ${project(p)}',
      (employeeId: final e?, projectId: null) => employee(e),
      (employeeId: null, projectId: final p?) => 'everyone in ${project(p)}',
      _ => 'a run',
    };

/// The editor's line under the counter: "Largest block: Bridge in aurelius · 18 940 / 16 000".
String largestLine(LargestBlock block, {required String Function(String) employee, required String Function(String) project}) =>
    'Largest block: ${contextName(block.context, employee: employee, project: project)} · ${counterLine(block.chars, max: blockMaxChars)}';

/// The refusal of B4 in words (`guideline_too_large`, or the check before Save):
/// "Bridge would get 25 300 characters in aurelius (max 16 000). Shorten this text or unlink something."
String tooLargeLine(GuidelineTooLarge t, {required String Function(String) employee, required String Function(String) project}) {
  final who = t.employeeId == null ? 'A run' : employee(t.employeeId!);
  final where = t.projectId == null ? '' : ' in ${project(t.projectId!)}';
  return '$who would get ${chars(t.chars)} characters$where (max ${chars(t.limit)}). Shorten this text or unlink something.';
}

/// What the phone says for a refused guideline write. [title]: the title that was taken.
String guidelineError(Object e, {String? title, required String Function(String) employee, required String Function(String) project}) => switch (e) {
      NotSentException() => 'Not sent. The PC was not reachable; nothing was saved.',
      ProtocolException(code: ErrorCode.guidelineTooLarge, :final tooLarge?) => tooLargeLine(tooLarge, employee: employee, project: project),
      ProtocolException(code: ErrorCode.guidelineTitleTaken) => title == null ? 'Another guideline has this title.' : 'Another guideline is called “$title” already.',
      ProtocolException(code: ErrorCode.looksLikeSecret, :final matchedItems) => secretInGuideline(matchedItems),
      ProtocolException(code: ErrorCode.versionConflict) => 'This guideline changed on another device. Showing the latest.',
      ProtocolException() => e.shown,
      _ => 'That did not work. Try again.',
    };

/// B10 for a guideline: never "Send anyway", because guidelines go into every run.
String secretInGuideline(List<String> items) =>
    'This text holds the value of ${items.isEmpty ? 'a vault item' : items.join(', ')}. Take the value out: guidelines go into every run, so they never hold a secret.';

/// "Applies from the next run of 4 employees." (B6), with how many conversations start a new session.
String appliesLine(GuidelineAffected? reach) {
  if (reach == null) return 'Applies from the next run.';
  final n = reach.employees;
  final who = n == 0 ? 'Applies from the next run; no employee gets it now.' : 'Applies from the next run of $n employee${n == 1 ? '' : 's'}.';
  final c = reach.conversations;
  return c == 0 ? who : '$who $c conversation${c == 1 ? '' : 's'} start${c == 1 ? 's' : ''} a new session.';
}

/// B6 under Save: "Runs that already started keep version 3."
String keepsVersionLine(int version) => 'Runs that already started keep version $version.';

/// "Guidelines: Rust v3, Git & worktrees v4" (P-59).
String? runGuidelinesLine(List<RunGuideline> list) => list.isEmpty ? null : 'Guidelines: ${list.map((g) => '${g.title} v${g.version}').join(', ')}';

/// How a version came about, in the words of the instructions (ui §4.2).
String sourceLabel(GuidelineVersionView v, {String? proposer}) => switch (v.source) {
      GuidelineSource.seed => 'starter set',
      GuidelineSource.created => 'created',
      GuidelineSource.edited => 'edited',
      GuidelineSource.proposal => proposer == null ? 'approved proposal' : 'approved proposal from $proposer',
      GuidelineSource.restored => v.restoredFrom == null ? 'restored' : 'restored from version ${v.restoredFrom}',
    };

/// "Flux, Bridge +2", "aurelius", "Not linked": who a guideline applies to.
String linksLine(List<GuidelineTarget> links, {required String Function(String) employee, required String Function(String) project, int shown = 2}) {
  if (links.isEmpty) return 'Not linked';
  final names = [
    for (final l in links)
      switch (l) {
        GuidelineTargetEmployee(:final id) => employee(id),
        GuidelineTargetProject(:final id) => project(id),
      },
  ];
  return names.length <= shown ? names.join(', ') : '${names.take(shown).join(', ')} +${names.length - shown}';
}

/// The list row's second line: "Flux, Bridge · 3 410 chars".
String librarySubtitle(GuidelineView g, {required String Function(String) employee, required String Function(String) project}) =>
    '${linksLine(g.links, employee: employee, project: project)} · ${chars(g.chars)} chars';

/// The banner after the seed (B7, ui §8.1), until Got it.
const seedBannerText = "Aurelius added a starter set with the team's current rules. Check them and their links; they apply from the next run.";

/// The empty library (ui §8.2).
const emptyLibraryTitle = 'No guidelines yet';
const emptyLibraryDetail =
    "Guidelines are working rules (how to write Rust, how to use worktrees) that you link to employees or projects. They come on top of an employee's instructions in every run.";
const emptyLibraryProposeHint = "Employees can also propose one; you'll find it in the Inbox.";

/// The note under the link list (P-54 with reports_to).
const linkGroupNote = "Selecting All links today's reports, not future ones.";

/// The seed made a guideline and nobody changed it since: its last version has no author.
bool fromStarterSet(GuidelineView g) => g.updatedBy == null && g.version == 1;

/// The link list's groups (B2, no departments): one per manager, by `reports_to`.
class ReportsGroup {
  const ReportsGroup({required this.managerId, required this.title, required this.employees});

  /// Null: the management team (nobody above them).
  final String? managerId;
  final String title;
  final List<EmployeeSummary> employees;
}

/// [employees] grouped by whom they report to, the management team first,
/// then by the manager's name; within a group by name. Archived ones stay
/// (GL-05 AC4), marked by the list.
List<ReportsGroup> reportsGroups(List<EmployeeSummary> employees) {
  final byId = {for (final e in employees) e.id: e};
  final groups = <String?, List<EmployeeSummary>>{};
  for (final e in employees) {
    final boss = e.reportsTo != null && byId.containsKey(e.reportsTo) ? e.reportsTo : null;
    groups.putIfAbsent(boss, () => []).add(e);
  }
  int byName(EmployeeSummary a, EmployeeSummary b) => a.name.toLowerCase().compareTo(b.name.toLowerCase());
  final keys = groups.keys.toList()
    ..sort((a, b) {
      if (a == null) return -1;
      if (b == null) return 1;
      return byName(byId[a]!, byId[b]!);
    });
  return [
    for (final k in keys)
      ReportsGroup(managerId: k, title: k == null ? 'Management team' : 'Reports to ${byId[k]!.name}', employees: groups[k]!..sort(byName)),
  ];
}

/// "(paused)", "(archived)" after a name in the link list.
String statusMark(EmployeeSummary e) => switch (e.status) {
      LifecycleStatus.paused => ' (paused)',
      LifecycleStatus.archived => ' (archived)',
      _ => '',
    };

/// The proposal card's kind (P-61).
String proposalKindLabel(GuidelineProposalView p) => p.guidelineId == null ? 'New guideline' : 'New guideline version';

/// "Proposed by Forge · v3 → 4", "Proposed by Forge".
String proposalByline(GuidelineProposalView p, String proposer) =>
    p.baseVersion == null ? 'Proposed by $proposer' : 'Proposed by $proposer · v${p.baseVersion} → ${p.baseVersion! + 1}';

/// The proposal rests on an older version than the guideline has now: it can only be rejected.
bool proposalStale(GuidelineProposalView p, GuidelineView? current) =>
    p.baseVersion != null && current != null && current.version != p.baseVersion;

/// The stale line (ui §7.1).
String staleLine(GuidelineProposalView p, GuidelineView current, String proposer) =>
    "“${current.title}” changed since this was proposed (version ${p.baseVersion} → ${current.version}), so it can't be approved. Reject it; $proposer can propose again on the new text.";

/// The links of a guideline as a set of keys, for comparing selections.
String targetKey(GuidelineTarget t) => switch (t) {
      GuidelineTargetEmployee(:final id) => 'e:$id',
      GuidelineTargetProject(:final id) => 'p:$id',
    };

GuidelineTarget targetOf(String key) => key.startsWith('e:') ? GuidelineTargetEmployee(id: key.substring(2)) : GuidelineTargetProject(id: key.substring(2));

/// Got it on the seed banner, per PC and board member (ui §8.1). Kept on the
/// phone only; every call falls back quietly (no preferences in tests).
abstract final class SeedBannerMemory {
  static String _key(String hostId, String userId) => 'guidelines.seed_banner.$hostId.$userId';

  static Future<bool> dismissed(String hostId, String userId) async {
    try {
      return (await SharedPreferences.getInstance()).getBool(_key(hostId, userId)) ?? false;
    } on Object {
      return false;
    }
  }

  static Future<void> dismiss(String hostId, String userId) async {
    try {
      await (await SharedPreferences.getInstance()).setBool(_key(hostId, userId), true);
    } on Object {
      // Shown again next time; nothing else depends on it.
    }
  }
}

/// A pending guideline proposal with what its card needs (GL-14 AC1): the
/// guideline as it is now (null for a new one) and the text of the version
/// the proposal rests on, for the diff.
class GuidelineProposalItem {
  const GuidelineProposalItem({required this.proposal, this.current, this.baseText});

  final GuidelineProposalView proposal;
  final GuidelineView? current;
  final String? baseText;

  String get id => proposal.id;
  bool get isNew => proposal.guidelineId == null;
  bool get stale => proposalStale(proposal, current);

  @override
  bool operator ==(Object other) =>
      other is GuidelineProposalItem && other.proposal.id == proposal.id && other.proposal.status == proposal.status && other.current?.version == current?.version && other.baseText == baseText;

  @override
  int get hashCode => Object.hash(proposal.id, proposal.status, current?.version, baseText);
}
