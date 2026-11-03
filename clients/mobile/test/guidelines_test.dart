import 'package:aurelius_mobile/app/mobile_core.dart';
import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/tokens.dart';
import 'package:aurelius_mobile/data/guidelines.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/guidelines/guideline_editor_page.dart';
import 'package:aurelius_mobile/features/guidelines/guideline_links_page.dart';
import 'package:aurelius_mobile/features/guidelines/guideline_markdown.dart';
import 'package:aurelius_mobile/features/guidelines/guideline_page.dart';
import 'package:aurelius_mobile/features/guidelines/guideline_proposal_card.dart';
import 'package:aurelius_mobile/features/guidelines/guidelines_bloc.dart';
import 'package:aurelius_mobile/features/guidelines/guidelines_page.dart';
import 'package:aurelius_mobile/features/guidelines/run_guidelines.dart';
import 'package:aurelius_mobile/features/guidelines/target_guidelines_page.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_sections.dart';
import 'package:aurelius_mobile/features/more/more_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Guidelines on the phone (AUR-GL-01-M1), against the frozen protocol: the
// library, reading (E14), the editor with 8 000 / 16 000 (orange from 80 %),
// links grouped by reports_to with boxes off over the limit, History and
// Restore, proposals in the Inbox, the run's "Guidelines: … vN", the empty
// state and the seed banner; edits for board members only. Neutral names.

const guidelinesLink = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: ['guidelines'],
  boardMembersExist: true,
  user: testUser,
  claudeLogin: testLogin,
);

/// Signed in, but only allowed to read (an app or a reader's phone): no edits, no decisions.
const readerLink = LinkState(status: LinkStatus.online, capabilities: ['read'], features: ['guidelines'], boardMembersExist: true, user: testUser, claudeLogin: testLogin);

const adaRef = UserRef(id: 'u-1', username: 'ada');
const robinRef = UserRef(id: 'u-2', username: 'robin');

GuidelineView guidelineOf(String id, String title, {int version = 1, int chars = 1000, List<GuidelineTarget> links = const [], int openProposals = 0, UserRef? by = adaRef, bool? archived}) => GuidelineView(
      archived: archived,
      chars: chars,
      id: id,
      links: links,
      openProposals: openProposals,
      title: title,
      updatedAtMs: 0,
      updatedBy: by,
      version: version,
    );

GuidelineVersionView versionOf(String id, int version, String text, {String title = 'Rust', GuidelineSource source = GuidelineSource.edited, UserRef? by = adaRef, int? restoredFrom}) =>
    GuidelineVersionView(by: by, createdAtMs: 0, guidelineId: id, restoredFrom: restoredFrom, source: source, text: text, title: title, version: version);

EffectiveGuideline effectiveOf(String id, String title, int chars, {int version = 1, GuidelineLevel level = GuidelineLevel.employee}) =>
    EffectiveGuideline(chars: chars, id: id, level: level, text: 'x' * chars, title: title, version: version);

EffectiveGuidelines setOf(List<EffectiveGuideline> list, {String? employeeId, String? projectId}) => EffectiveGuidelines(
      employeeId: employeeId,
      guidelines: list,
      limitChars: blockMaxChars,
      projectId: projectId,
      totalChars: list.fold(0, (n, g) => n + g.chars),
    );

GuidelineProposalView proposalOfGuideline(String id, {String? guidelineId = 'g-rust', int? baseVersion = 3, String title = 'Rust', String text = '## Async\n- Never block in async code.', String by = 'forge', List<GuidelineTarget> suggested = const []}) =>
    GuidelineProposalView(
      baseVersion: baseVersion,
      createdAtMs: 0,
      evidence: const [ProposalEvidence(id: 't7', kind: 'task', label: 'Review found blocking I/O')],
      guidelineId: guidelineId,
      id: id,
      proposedBy: by,
      reason: 'Twice this week a review found blocking I/O.',
      status: ProposalStatus.pending,
      suggestedLinks: suggested,
      text: text,
      title: title,
    );

// The team: two managers, three reports.
final titan = employeeOf(id: 'titan', name: 'Titan', role: 'Tech Lead');
final orionM = employeeOf(id: 'orion', name: 'Orion', role: 'Project Manager');
final forgeE = employeeOf(id: 'forge', name: 'Forge', role: 'Backend Developer', reportsTo: 'titan', projectsActive: const ['p1']);
final bridgeE = employeeOf(id: 'bridge', name: 'Bridge', role: 'Fullstack Developer', reportsTo: 'titan');
final pulseE = employeeOf(id: 'pulse', name: 'Pulse', role: 'QA Engineer', reportsTo: 'orion', status: LifecycleStatus.paused);
final team = [titan, orionM, forgeE, bridgeE, pulseE];
CompanyState teamCompany() => companyWith(employees: team);

final rust = guidelineOf('g-rust', 'Rust', version: 3, chars: 6120, links: const [GuidelineTargetEmployee(id: 'forge'), GuidelineTargetEmployee(id: 'bridge')], openProposals: 1);
final git = guidelineOf('g-git', 'Git & worktrees', version: 4, chars: 2050, links: const [GuidelineTargetProject(id: 'p1')]);
final docs = guidelineOf('g-docs', 'Writing docs', chars: 1200);
final oldOne = guidelineOf('g-old', 'Old rules', chars: 300, archived: true);

/// A repository that answers the guideline reads; every write succeeds.
MockRepository guidelineRepository({
  List<GuidelineView>? active,
  List<GuidelineView> archived = const [],
  List<GuidelineProposalView> proposals = const [],
  Map<(String?, String?), EffectiveGuidelines> sets = const {},
  LinkState link = guidelinesLink,
  Stream<Event>? changes,
}) {
  final repo = boardRepository();
  when(() => repo.linkState).thenReturn(link);
  when(() => repo.guidelineChanges).thenAnswer((_) => changes ?? const Stream.empty());
  when(() => repo.guidelines(archived: false)).thenAnswer((_) async => active ?? [git, rust, docs]);
  when(() => repo.guidelines(archived: true)).thenAnswer((_) async => archived);
  when(() => repo.guidelineProposalList(status: any(named: 'status'))).thenAnswer((_) async => proposals);
  when(() => repo.effectiveGuidelines(employeeId: any(named: 'employeeId'), projectId: any(named: 'projectId'))).thenAnswer((i) async {
    final e = i.namedArguments[#employeeId] as String?;
    final p = i.namedArguments[#projectId] as String?;
    return sets[(e, p)] ?? setOf(const [], employeeId: e, projectId: p);
  });
  when(() => repo.guidelineReach(any())).thenAnswer((i) async => GuidelineAffected(conversations: 1, employees: 3, guidelineId: i.positionalArguments.first as String, projects: 1));
  when(() => repo.guideline(any(), version: any(named: 'version'))).thenAnswer((i) async {
    final id = i.positionalArguments.first as String;
    final g = [...?active, ...archived, rust, git, docs].firstWhere((g) => g.id == id);
    return GuidelineDetail(guideline: g, version: versionOf(id, (i.namedArguments[#version] as int?) ?? g.version, '## Errors\n- No unwrap outside tests.', title: g.title));
  });
  when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
      .thenAnswer((_) async => receiptOf(const CommandResultGuidelineChanged(guidelineId: 'g-new', version: 1)));
  return repo;
}

/// [page] with a [GuidelinesBloc] of [repo], under the board test router; returns the bloc.
Future<GuidelinesBloc> pumpWithGuidelines(WidgetTester tester, MockRepository repo, Widget page, {LinkState link = guidelinesLink, CompanyState? company}) async {
  // Made in the tree, not with addTearDown(bloc.close): that hangs a testWidgets (C2c).
  await pumpBoardPage(tester, repo, BlocProvider(create: (_) => GuidelinesBloc(repo), child: page), link: link, company: company ?? teamCompany());
  await settleBoard(tester);
  return tester.element(find.byWidget(page)).read<GuidelinesBloc>();
}

List<Map<String, dynamic>> guidelineWrites(MockRepository repo) => writesOf(repo).where((w) => (w['method'] as String).startsWith('guideline.')).toList();

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(ProposalStatus.pending);
  });

  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('the limits and texts without widgets', () {
    test('B4: 8 000 per guideline; orange from 80 %, red over the limit, always with words', () {
      expect(limitLevel(6399, guidelineMaxChars), LimitLevel.ok);
      expect(limitLevel(6400, guidelineMaxChars), LimitLevel.close);
      expect(limitLevel(8000, guidelineMaxChars), LimitLevel.close);
      expect(limitLevel(8001, guidelineMaxChars), LimitLevel.over);
      expect(limitLevel(12800, blockMaxChars), LimitLevel.close);
      expect(limitLevel(16001, blockMaxChars), LimitLevel.over);
      expect(counterLine(6120), '6 120 / 8 000');
      expect(limitNote(6000), isNull);
      expect(limitNote(6500), 'Close to the limit');
      expect(limitNote(9240), '1 240 characters over the limit of 8 000');
      expect(counterLabel(6120), '6 120 of 8 000 characters');
      expect(blockMaxChars, 16000, reason: 'the correction at the start of AUR-GL-01: 16 000 per run, not 24 000');
    });

    test('the counter counts code points, like the server (chars().count())', () {
      expect(charCount('Rust 🦀'), 6);
      expect(charCount('é'), 1);
    });

    test('the link list groups by reports_to (no departments): the management team first, then per manager; paused ones stay, marked', () {
      final groups = reportsGroups(team);
      expect(groups.map((g) => g.title), ['Management team', 'Reports to Orion', 'Reports to Titan']);
      expect(groups.first.employees.map((e) => e.name), ['Orion', 'Titan']);
      expect(groups.last.employees.map((e) => e.name), ['Bridge', 'Forge']);
      expect(statusMark(pulseE), ' (paused)');
      // A manager the phone does not know puts the report on top.
      expect(reportsGroups([employeeOf(id: 'x', name: 'Xan', reportsTo: 'gone')]).single.title, 'Management team');
    });

    test('P-56: a link reaches the contexts the server checks (combinations_over): every project, with or without open work; archived ones not', () {
      final projects = [
        projectOf(),
        projectOf(id: 'p2', name: 'Paused', status: ProjectStatus.paused),
        projectOf(id: 'p3', name: 'Old', status: ProjectStatus.archived),
      ];
      final withGone = [...team, employeeOf(id: 'gone', name: 'Gone', status: LifecycleStatus.archived)];
      // Bridge has no open work anywhere: its block in each project counts all the same (Pulse M2).
      expect(contextsOf(const GuidelineTargetEmployee(id: 'bridge'), withGone, projects), [
        (employeeId: 'bridge', projectId: null),
        (employeeId: 'bridge', projectId: 'p1'),
        (employeeId: 'bridge', projectId: 'p2'),
      ]);
      expect(contextsOf(const GuidelineTargetProject(id: 'p1'), withGone, projects), [
        (employeeId: null, projectId: 'p1'),
        for (final e in team) (employeeId: e.id, projectId: 'p1'),
      ]);
      expect(contextsOf(const GuidelineTargetEmployee(id: 'gone'), withGone, projects), isEmpty);
      expect(contextsOf(const GuidelineTargetProject(id: 'p3'), withGone, projects), isEmpty);
      expect(contextsOfAll(const [GuidelineTargetEmployee(id: 'bridge'), GuidelineTargetProject(id: 'p1')], withGone, projects), hasLength(8));
      // The employee's own page still shows the projects it works in (GL-07).
      expect(workContextsOf(forgeE, 'forge'), [(employeeId: 'forge', projectId: null), (employeeId: 'forge', projectId: 'p1')]);
    });

    test('a guideline already in a block counts once (B3, P-55); the largest block wins', () {
      final set = setOf([effectiveOf('g-rust', 'Rust', 6000), effectiveOf('g-git', 'Git', 2000)]);
      expect(blockWith(set, 'g-rust', 7000), 9000);
      expect(blockWith(set, 'g-docs', 1000), 9000);
      expect(blockWith(set, 'g-rust', null), 2000);
      const a = (employeeId: 'forge', projectId: null);
      const b = (employeeId: 'forge', projectId: 'p1');
      expect(largestOf([(a, 9000), (b, 17000)]), (chars: 17000, context: b));
      expect(largestOf(const []), isNull);
    });

    test('refusals in words: too large names who and where, a taken title, a vault value without Send anyway, a conflict', () {
      String e(String id) => {'forge': 'Forge'}[id] ?? id;
      String p(String id) => {'p1': 'Atlas'}[id] ?? id;
      const big = ProtocolException(ErrorCode.guidelineTooLarge, 'too large', tooLarge: GuidelineTooLarge(chars: 25300, limit: 16000, employeeId: 'forge', projectId: 'p1'));
      expect(guidelineError(big, employee: e, project: p), 'Forge would get 25 300 characters in Atlas (max 16 000). Shorten this text or unlink something.');
      expect(guidelineError(const ProtocolException(ErrorCode.guidelineTitleTaken, 'x'), title: 'Rust', employee: e, project: p), 'Another guideline is called “Rust” already.');
      final secret = guidelineError(const ProtocolException(ErrorCode.looksLikeSecret, 'x', matchedItems: ['GitHub token']), employee: e, project: p);
      expect(secret, contains('GitHub token'));
      expect(secret, isNot(contains('anyway')));
      expect(guidelineError(const ProtocolException(ErrorCode.versionConflict, 'x'), employee: e, project: p), contains('changed on another device'));
      expect(guidelineError(const NotSentException('k'), employee: e, project: p), contains('nothing was saved'));
    });

    test('a placeholder server ("not available on this server yet") counts as not on the server', () {
      expect(const ProtocolException(ErrorCode.validation, 'Guidelines are not available on this server yet.').notOnServer, isTrue);
      expect(const ProtocolException(ErrorCode.validation, 'Title is empty').notOnServer, isFalse);
    });

    test('P-59: the run line names title and version in block order; nothing for a run without guidelines', () {
      const list = [
        RunGuideline(chars: 1, guidelineId: 'g-rust', level: GuidelineLevel.employee, runId: 'r1', title: 'Rust', version: 3),
        RunGuideline(chars: 1, guidelineId: 'g-git', level: GuidelineLevel.project, runId: 'r1', title: 'Git & worktrees', version: 4),
      ];
      expect(runGuidelinesLine(list), 'Guidelines: Rust v3, Git & worktrees v4');
      expect(runGuidelinesLine(const []), isNull);
    });

    test('B6: "applies from the next run of N employees", and the sessions that start anew', () {
      expect(appliesLine(const GuidelineAffected(conversations: 0, employees: 4, guidelineId: 'g', projects: 0)), 'Applies from the next run of 4 employees.');
      expect(appliesLine(const GuidelineAffected(conversations: 2, employees: 1, guidelineId: 'g', projects: 0)), 'Applies from the next run of 1 employee. 2 conversations start a new session.');
      expect(keepsVersionLine(3), 'Runs that already started keep version 3.');
    });

    test('a version names how it came about, in the words of the instructions', () {
      expect(sourceLabel(versionOf('g', 1, 'x', source: GuidelineSource.seed, by: null)), 'starter set');
      expect(sourceLabel(versionOf('g', 4, 'x', source: GuidelineSource.restored, restoredFrom: 2)), 'restored from version 2');
      expect(sourceLabel(versionOf('g', 3, 'x', source: GuidelineSource.proposal), proposer: 'Forge'), 'approved proposal from Forge');
    });

    test('a proposal on an older version is stale; the seed is a version 1 without author', () {
      expect(proposalStale(proposalOfGuideline('gp1', baseVersion: 3), rust), isFalse);
      expect(proposalStale(proposalOfGuideline('gp1', baseVersion: 2), rust), isTrue);
      expect(proposalStale(proposalOfGuideline('gp1', guidelineId: null, baseVersion: null), null), isFalse);
      expect(fromStarterSet(guidelineOf('g', 'Rust', by: null)), isTrue);
      expect(fromStarterSet(guidelineOf('g', 'Rust', by: null, version: 2)), isFalse);
      expect(fromStarterSet(guidelineOf('g', 'Rust')), isFalse);
    });
  });

  group('GuidelinesBloc', () {
    test('reads the active, the archived and the pending proposals; a guideline event reads again', () async {
      final events = StreamController<Event>.broadcast();
      final repo = guidelineRepository(archived: [oldOne], proposals: [proposalOfGuideline('gp1')], changes: events.stream);
      final bloc = GuidelinesBloc(repo);
      await stateWhere(bloc, (s) => s.active != null);
      expect(bloc.state.shown.map((g) => g.title), ['Git & worktrees', 'Rust', 'Writing docs']);
      expect(bloc.state.archived.single.title, 'Old rules');
      expect(bloc.state.proposalsFor('g-rust'), hasLength(1));
      bloc.add(const GuidelinesQueryChanged('ru'));
      await stateWhere(bloc, (s) => s.query == 'ru');
      expect(bloc.state.shown.single.title, 'Rust');
      events.add(const Event(seq: 2, kind: EventKind.guidelineUpdated, atMs: 0, aggregateId: 'g-rust'));
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verify(() => repo.guidelines(archived: false)).called(2);
      await bloc.close();
      await events.close();
    });

    test('board members only: without the right to change work nothing is editable (B5)', () async {
      expect(GuidelinesBloc(guidelineRepository()).canEdit, isTrue);
      expect(GuidelinesBloc(guidelineRepository(link: readerLink)).canEdit, isFalse);
      expect(GuidelinesBloc(guidelineRepository(link: guidelinesLink.withBoardMember(exists: true))).canEdit, isFalse);
    });

    test('update sends what changed with the version the editor started at; links go one by one', () async {
      final repo = guidelineRepository();
      final bloc = GuidelinesBloc(repo);
      await bloc.update(rust, text: 'new', fromVersion: 3);
      final update = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 3, retry: false)).captured.single as Map<String, dynamic>;
      expect(update['method'], 'guideline.update');
      expect(paramsOf(update), {'guideline_id': 'g-rust', 'text': 'new'});
      await bloc.setLinks('g-rust', {'e:forge', 'e:bridge'}, {'e:forge', 'p:p1'});
      final links = guidelineWrites(repo);
      expect(links.map((w) => w['method']), ['guideline.link', 'guideline.unlink']);
      expect(paramsOf(links[0])['target'], {'kind': 'project', 'id': 'p1'});
      expect(paramsOf(links[1])['target'], {'kind': 'employee', 'id': 'bridge'});
      await bloc.close();
    });

    test('B7: the seed banner shows while the starter set is untouched, until Got it (per board member)', () async {
      final repo = guidelineRepository(active: [guidelineOf('g-s', 'Rust', by: null)]);
      final bloc = GuidelinesBloc(repo);
      await stateWhere(bloc, (s) => s.active != null);
      expect(bloc.state.showSeedBanner, isTrue);
      bloc.add(const GuidelinesBannerDismissed());
      await stateWhere(bloc, (s) => s.bannerDismissed);
      expect(await SeedBannerMemory.dismissed('h', 'u-1'), isTrue);
      final again = GuidelinesBloc(repo);
      await stateWhere(again, (s) => s.active != null);
      expect(again.state.showSeedBanner, isFalse);
      await bloc.close();
      await again.close();
    });
  });

  group('guideline proposals in the Inbox', () {
    MockRepository inboxRepository({LinkState link = guidelinesLink, List<GuidelineProposalView>? proposals, StreamController<Event>? changes}) {
      final repo = guidelineRepository(link: link, proposals: proposals ?? [proposalOfGuideline('gp1'), proposalOfGuideline('gp2', guidelineId: null, baseVersion: null, title: 'Async Rust', suggested: const [GuidelineTargetEmployee(id: 'forge')])]);
      when(() => repo.companyChanges).thenAnswer((_) => changes?.stream ?? const Stream.empty());
      when(() => repo.approvals()).thenAnswer((_) async => <ApprovalView>[]);
      when(() => repo.notifications()).thenAnswer((_) async => const NotificationsSnapshot(notifications: [], unread: 0));
      when(() => repo.proposals()).thenAnswer((_) async => <ProposalView>[]);
      when(() => repo.guideline('g-rust', version: 3)).thenAnswer((_) async => GuidelineDetail(guideline: rust, version: versionOf('g-rust', 3, '## Errors\n- No unwrap.')));
      return repo;
    }

    test('only on a server with guidelines; the diff rests on the base version; they count for the badge (GL-14 AC1)', () async {
      final repo = inboxRepository();
      final bloc = InboxBloc(repo);
      await stateWhere(bloc, (s) => s.guidelineProposals.length == 2);
      final change = bloc.state.guidelineProposals.firstWhere((g) => !g.isNew);
      expect(change.baseText, '## Errors\n- No unwrap.');
      expect(change.current?.version, 3);
      expect(bloc.state.badge, 2);
      expect(InboxSections.of(bloc.state, teamCompany()).needsYou, 2);
      await bloc.close();

      final old = inboxRepository(link: onlineLink);
      final oldBloc = InboxBloc(old);
      await Future<void>.delayed(const Duration(milliseconds: 400));
      verifyNever(() => old.guidelineProposalList(status: any(named: 'status')));
      expect(oldBloc.state.guidelineProposals, isEmpty);
      await oldBloc.close();
    });

    test('they are read again on a guideline event only, not on every change of the company (C2c)', () async {
      final changes = StreamController<Event>.broadcast();
      final repo = inboxRepository(changes: changes);
      final bloc = InboxBloc(repo);
      await stateWhere(bloc, (s) => s.guidelineProposals.isNotEmpty);
      changes.add(const Event(seq: 2, kind: EventKind.taskCreated, atMs: 0, aggregateId: 't1'));
      await Future<void>.delayed(const Duration(milliseconds: 400));
      verify(() => repo.guidelineProposalList(status: any(named: 'status'))).called(1);
      changes.add(const Event(seq: 3, kind: EventKind.guidelineProposed, atMs: 0, aggregateId: 'gp3'));
      await Future<void>.delayed(const Duration(milliseconds: 400));
      verify(() => repo.guidelineProposalList(status: any(named: 'status'))).called(1);
      await bloc.close();
      await changes.close();
    });

    test('approve with an edited text, a new one with the chosen links, reject with a note (GL-14 AC2, AC4)', () async {
      final repo = inboxRepository();
      final bloc = InboxBloc(repo);
      await stateWhere(bloc, (s) => s.guidelineProposals.length == 2);
      final change = bloc.state.guidelineProposals.firstWhere((g) => !g.isNew);
      final fresh = bloc.state.guidelineProposals.firstWhere((g) => g.isNew);
      bloc.add(GuidelineProposalDecided(change, approve: true, text: 'shorter', proposer: 'Forge'));
      await stateWhere(bloc, (s) => s.notice != null);
      expect(bloc.state.notice, 'Approved. The new version of “Rust” applies from the next run.');
      bloc.add(GuidelineProposalDecided(fresh, approve: true, links: const [GuidelineTargetProject(id: 'p1')], proposer: 'Forge'));
      await stateWhere(bloc, (s) => s.notice?.contains('Async Rust') ?? false);
      bloc.add(GuidelineProposalDecided(change, approve: false, note: 'Too strict', proposer: 'Forge'));
      await stateWhere(bloc, (s) => s.notice?.startsWith('Rejected') ?? false);
      expect(bloc.state.notice, 'Rejected. Forge gets your note.');
      final w = guidelineWrites(repo).map(paramsOf).toList();
      expect(w[0], {'approve': true, 'proposal_id': 'gp1', 'text': 'shorter'});
      expect(w[1], {
        'approve': true,
        'proposal_id': 'gp2',
        'links': [
          {'kind': 'project', 'id': 'p1'},
        ],
      });
      expect(w[2], {'approve': false, 'proposal_id': 'gp1', 'note': 'Too strict'});
      await bloc.close();
    });

    test('a refused approval keeps the proposal with the reason: version_conflict, too large (GL-14 AC3, AC5)', () async {
      final repo = inboxRepository();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.versionConflict, 'conflict'));
      final bloc = InboxBloc(repo);
      await stateWhere(bloc, (s) => s.guidelineProposals.length == 2);
      final change = bloc.state.guidelineProposals.firstWhere((g) => !g.isNew);
      bloc.add(GuidelineProposalDecided(change, approve: true, proposer: 'Forge'));
      await stateWhere(bloc, (s) => s.notice != null);
      expect(bloc.state.notice, contains("changed since this was proposed, so it can't be approved"));
      bloc.add(const InboxNoticeShown());
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenThrow(
          const ProtocolException(ErrorCode.guidelineTooLarge, 'x', tooLarge: GuidelineTooLarge(chars: 19230, limit: 16000, employeeId: 'bridge', projectId: 'p1')));
      bloc.add(GuidelineProposalDecided(change, approve: true, proposer: 'Forge', employee: (_) => 'Bridge', project: (_) => 'Atlas'));
      await stateWhere(bloc, (s) => s.notice?.contains('19') ?? false);
      expect(bloc.state.notice, 'Bridge would get 19 230 characters in Atlas (max 16 000). Shorten this text or unlink something.');
      await bloc.close();
    });

    test('E15: the guideline.proposed notification is not a second Needs you item (the card is)', () {
      const n = NotificationView(id: 'n1', severity: 'action', title: 'Forge proposes a guideline', subject: 'Rust', createdAtMs: 0, read: false, targetType: 'guideline.proposed');
      expect(n.asksYou, isFalse);
      expect(n.givesBadge, isFalse);
    });
  });

  group('the screens', () {
    testWidgets('More → Guidelines only on a server that has them', (tester) async {
      mobileCoreHidden = const {};
      addTearDown(() => mobileCoreHidden = kHiddenInMobileCore);
      final repo = guidelineRepository();
      await pumpBoardPage(tester, repo, const MorePage(), link: guidelinesLink);
      expect(find.byKey(const ValueKey('more-guidelines')), findsOneWidget);
      await pumpBoardPage(tester, repo, const MorePage());
      expect(find.byKey(const ValueKey('more-guidelines')), findsNothing);
    });

    testWidgets('the library: version, links and size per row, ◆ for a proposal, archived apart, + for board members (GL-01 AC1, GL-04 AC2)', (tester) async {
      final repo = guidelineRepository(archived: [oldOne]);
      await pumpBoardPage(tester, repo, const GuidelinesPage(), link: guidelinesLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text('Rust'), findsOneWidget);
      expect(find.text('v3'), findsOneWidget);
      expect(find.text('Forge, Bridge · 6 120 chars'), findsOneWidget);
      expect(find.text('Atlas · 2 050 chars'), findsOneWidget);
      expect(find.text('Not linked · 1 200 chars'), findsOneWidget);
      expect(find.text('◆ 1'), findsOneWidget);
      expect(find.byKey(const ValueKey('guideline-new')), findsOneWidget);
      expect(find.text('Old rules', skipOffstage: false), findsOneWidget);
      expect(find.byKey(const ValueKey('guideline-archived-header'), skipOffstage: false), findsOneWidget);
    });

    testWidgets('a reader sees the library without + and without swipe to archive', (tester) async {
      final repo = guidelineRepository(link: readerLink);
      await pumpBoardPage(tester, repo, const GuidelinesPage(), link: readerLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text('Rust'), findsOneWidget);
      expect(find.byKey(const ValueKey('guideline-new')), findsNothing);
      expect(find.byType(Dismissible), findsNothing);
    });

    testWidgets('a swipe archives after a question that names the links (GL-04 AC1)', (tester) async {
      final repo = guidelineRepository();
      await pumpBoardPage(tester, repo, const GuidelinesPage(), link: guidelinesLink, company: teamCompany());
      await settleBoard(tester);
      await tester.drag(find.byKey(const ValueKey('guideline-swipe-g-rust')), const Offset(-500, 0));
      await settleBoard(tester);
      expect(find.text('Archive Rust?'), findsOneWidget);
      expect(find.textContaining('It is linked to Forge, Bridge'), findsOneWidget);
      await tester.tap(find.widgetWithText(TextButton, 'Archive').last);
      await settleBoard(tester);
      expect(guidelineWrites(repo).single['method'], 'guideline.archive');
    });

    testWidgets('empty: the text, New guideline as FilledButton and Add starter set as a text button (ui §8.2)', (tester) async {
      final repo = guidelineRepository(active: const []);
      when(() => repo.starterTemplates()).thenAnswer((_) async => const [
            StarterTemplate(chars: 3200, key: 'rust_style', level: GuidelineLevel.employee, text: 'x', title: 'Rust'),
            StarterTemplate(chars: 900, key: 'worktrees_build', level: GuidelineLevel.project, text: 'x', title: 'Git & worktrees', inLibrary: true),
          ]);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultStarterGuidelinesAdded(guidelineIds: ['g-1'])));
      await pumpBoardPage(tester, repo, const GuidelinesPage(), link: guidelinesLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text(emptyLibraryTitle), findsOneWidget);
      expect(find.widgetWithText(FilledButton, 'New guideline'), findsOneWidget);
      expect(find.widgetWithText(TextButton, 'Add starter set'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('guideline-empty-starter')));
      await settleBoard(tester);
      expect(find.text('Already in the library'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('starter-add')));
      await settleBoard(tester);
      expect(paramsOf(guidelineWrites(repo).single), {'keys': ['rust_style']});
      expect(find.text('Added 1 guideline. Link them to employees to use them.'), findsOneWidget);
    });

    testWidgets('empty for a reader: no buttons, the hint that employees can propose', (tester) async {
      await pumpBoardPage(tester, guidelineRepository(active: const [], link: readerLink), const GuidelinesPage(), link: readerLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text(emptyLibraryTitle), findsOneWidget);
      expect(find.byKey(const ValueKey('guideline-empty-new')), findsNothing);
      expect(find.text(emptyLibraryProposeHint), findsOneWidget);
    });

    testWidgets('the seed banner with Got it (ui §8.1)', (tester) async {
      final repo = guidelineRepository(active: [guidelineOf('g-s', 'Rust', by: null)]);
      await pumpBoardPage(tester, repo, const GuidelinesPage(), link: guidelinesLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text(seedBannerText), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('guideline-seed-got-it')));
      await settleBoard(tester);
      expect(find.text(seedBannerText), findsNothing);
    });

    testWidgets('a guideline to read: version, applies to, "next run of N employees", the proposal bar and the menu for board members', (tester) async {
      final repo = guidelineRepository(proposals: [proposalOfGuideline('gp1')]);
      await pumpWithGuidelines(tester, repo, const GuidelinePage(guidelineId: 'g-rust'));
      expect(find.textContaining('Version 3 · '), findsOneWidget);
      expect(find.text('Forge, Bridge'), findsOneWidget);
      expect(find.text('Applies from the next run of 3 employees. 1 conversation starts a new session.'), findsOneWidget);
      expect(find.text('Forge proposes a change'), findsOneWidget);
      expect(find.text('Errors'), findsOneWidget, reason: 'the markdown is rendered');
      expect(find.byTooltip('More'), findsOneWidget);
    });

    testWidgets('a run\'s version opens read-only and says it is not the current one (GL-11 AC3)', (tester) async {
      final repo = guidelineRepository();
      await pumpWithGuidelines(tester, repo, const GuidelinePage(guidelineId: 'g-rust', version: 2));
      expect(find.text('Version 2, as a run had it. The current version is 3.'), findsOneWidget);
      expect(find.byTooltip('More'), findsNothing);
    });

    testWidgets('History: versions with who and how, a diff, Restore after a question (GL-02 AC2, GL-03 AC1, P-58)', (tester) async {
      final repo = guidelineRepository();
      when(() => repo.guidelineVersionList('g-rust')).thenAnswer((_) async => [
            versionOf('g-rust', 3, 'A\nC', source: GuidelineSource.proposal),
            versionOf('g-rust', 2, 'A\nB', by: robinRef),
            versionOf('g-rust', 1, 'A', source: GuidelineSource.seed, by: null),
          ]);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultGuidelineChanged(guidelineId: 'g-rust', version: 4)));
      await pumpWithGuidelines(tester, repo, const GuidelinePage(guidelineId: 'g-rust'));
      await tester.tap(find.byKey(const ValueKey('guideline-history-row')));
      await settleBoard(tester);
      expect(find.textContaining('Version 3 · current · ada'), findsOneWidget);
      expect(find.text('starter set'), findsOneWidget);
      expect(find.text('approved proposal'), findsOneWidget);
      await tester.tap(find.textContaining('Version 2 · robin'));
      await settleBoard(tester);
      expect(find.text('− B'), findsOneWidget);
      expect(find.text('+ C'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('restore-2')));
      await settleBoard(tester);
      expect(find.text('Restore version 2 of Rust?'), findsOneWidget);
      expect(find.text('This becomes version 4 and applies from the next run.'), findsOneWidget);
      await tester.tap(find.widgetWithText(TextButton, 'Restore').last);
      await settleBoard(tester);
      final w = guidelineWrites(repo).single;
      expect(w['method'], 'guideline.restore_version');
      expect(paramsOf(w), {'guideline_id': 'g-rust', 'version': 2});
    });

    group('the editor', () {
      Future<List<(String, String)>> openEditor(WidgetTester tester, {String text = '', Object? fail, List<GuidelineTarget> links = const [], Map<(String?, String?), EffectiveGuidelines> sets = const {}, GuidelineView? guideline}) async {
        final saved = <(String, String)>[];
        final repo = guidelineRepository(sets: sets);
        await pumpWithGuidelines(
          tester,
          repo,
          GuidelineEditorPage(
            guideline: guideline,
            fromVersion: guideline?.version,
            initialTitle: guideline?.title ?? '',
            initialText: text,
            links: links,
            onSave: (t, x) async {
              if (fail != null) throw fail;
              saved.add((t, x));
            },
          ),
        );
        return saved;
      }

      testWidgets('the counter: neutral, orange from 80 % with words, over 8 000 Save is off (B4, GL-12 AC4)', (tester) async {
        await openEditor(tester);
        await tester.enterText(find.byKey(const ValueKey('guideline-title')), 'Rust');
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'x' * 6000);
        await tester.pump();
        expect(find.text('6 000 / 8 000'), findsOneWidget);
        expect(find.text('Close to the limit'), findsNothing);
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('guideline-save'))).onPressed, isNotNull);
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'x' * 6400);
        await tester.pump();
        expect(find.text('Close to the limit'), findsOneWidget);
        final bar = tester.widget<LinearProgressIndicator>(find.descendant(of: find.byKey(const ValueKey('guideline-counter')), matching: find.byType(LinearProgressIndicator)));
        expect(bar.color, isNot(bar.backgroundColor));
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'x' * 8001);
        await tester.pump();
        // Once, at the counter; not again as the reason under it (Pulse m6).
        expect(find.text('1 characters over the limit of 8 000'), findsOneWidget);
        expect(find.byKey(const ValueKey('guideline-editor-error')), findsNothing);
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('guideline-save'))).onPressed, isNull);
        await settleBoard(tester);
      });

      testWidgets('the largest block over its links against 16 000; over it Save is off with who and where (B4, P-56)', (tester) async {
        final sets = {
          ('forge', null): setOf([effectiveOf('g-git', 'Git', 2000)]),
          ('forge', 'p1'): setOf([effectiveOf('g-git', 'Git', 2000, level: GuidelineLevel.project), effectiveOf('g-big', 'Big', 10000)]),
        };
        await openEditor(tester, guideline: rust, text: 'x' * 3000, links: const [GuidelineTargetEmployee(id: 'forge')], sets: sets);
        expect(find.text('Largest block: Forge in Atlas · 15 000 / 16 000'), findsOneWidget);
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'x' * 5000);
        await tester.pump();
        expect(find.text('Forge would get 17 000 characters in Atlas (max 16 000). Shorten this text or unlink something.'), findsOneWidget);
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('guideline-save'))).onPressed, isNull);
        expect(find.text('Applies from the next run. Runs that already started keep version 3.'), findsOneWidget);
      });

      testWidgets('Pulse M2: the block in a project counts without open work there; 14 473 / 16 000 is orange', (tester) async {
        // Bridge has 5 047 of its own and P1 + P2 (2 458) through the project, but no task in it.
        expect(bridgeE.projectsActive, isEmpty);
        final sets = {
          ('bridge', null): setOf([effectiveOf('g-own', 'Own rules', 5047)]),
          ('bridge', 'p1'): setOf([effectiveOf('g-own', 'Own rules', 5047), effectiveOf('g-p', 'P1 and P2', 2458, level: GuidelineLevel.project)]),
        };
        await openEditor(tester, guideline: guidelineOf('g-large-a', 'Large rules A', chars: 6968), text: 'x' * 6968, links: const [GuidelineTargetEmployee(id: 'bridge')], sets: sets);
        expect(find.text('Largest block: Bridge in Atlas · 14 473 / 16 000'), findsOneWidget);
        final block = find.byKey(const ValueKey('guideline-block'));
        expect(find.descendant(of: block, matching: find.text('Close to the limit')), findsOneWidget);
        final bar = tester.widget<LinearProgressIndicator>(find.descendant(of: block, matching: find.byType(LinearProgressIndicator)));
        expect(bar.color, Tokens.warning);
      });

      testWidgets('Preview renders the markdown; nothing changed means no Save (GL-02 AC5)', (tester) async {
        await openEditor(tester, guideline: rust, text: '## Errors\n- No unwrap.');
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('guideline-save'))).onPressed, isNull);
        await tester.tap(find.text('Preview'));
        await tester.pump();
        expect(find.byKey(const ValueKey('guideline-preview')), findsOneWidget);
        expect(find.text('Errors'), findsOneWidget);
        await settleBoard(tester);
      });

      testWidgets('a taken title shows under the field; too large and a vault value show with the reason; the editor stays open', (tester) async {
        await openEditor(tester, fail: const ProtocolException(ErrorCode.guidelineTitleTaken, 'taken'));
        await tester.enterText(find.byKey(const ValueKey('guideline-title')), 'Rust');
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'Some rules');
        await tester.pump();
        await tester.tap(find.byKey(const ValueKey('guideline-save')));
        await settleBoard(tester);
        expect(find.text('Another guideline is called “Rust” already.'), findsOneWidget);
        expect(find.byKey(const ValueKey('guideline-text')), findsOneWidget);
      });

      testWidgets('looks_like_secret: the item names, no Send anyway (B10)', (tester) async {
        await openEditor(tester, fail: const ProtocolException(ErrorCode.looksLikeSecret, 'secret', matchedItems: ['GitHub token']));
        await tester.enterText(find.byKey(const ValueKey('guideline-title')), 'Deploy');
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'token here');
        await tester.pump();
        await tester.tap(find.byKey(const ValueKey('guideline-save')));
        await settleBoard(tester);
        expect(find.textContaining('holds the value of GitHub token'), findsOneWidget);
        expect(find.textContaining('anyway'), findsNothing);
      });

      testWidgets('guideline_too_large from the server names the combination (GL-12 AC2)', (tester) async {
        await openEditor(tester, fail: const ProtocolException(ErrorCode.guidelineTooLarge, 'x', tooLarge: GuidelineTooLarge(chars: 25300, limit: 16000, employeeId: 'bridge', projectId: 'p1')));
        await tester.enterText(find.byKey(const ValueKey('guideline-title')), 'Big');
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'rules');
        await tester.pump();
        await tester.tap(find.byKey(const ValueKey('guideline-save')));
        await settleBoard(tester);
        expect(find.text('Bridge would get 25 300 characters in Atlas (max 16 000). Shorten this text or unlink something.'), findsOneWidget);
      });

      testWidgets('version_conflict: who saved which version, Compare and Discard mine; never overwritten (GL-02 AC4, P-64)', (tester) async {
        final repo = guidelineRepository();
        when(() => repo.guideline('g-rust', version: null)).thenAnswer((_) async => GuidelineDetail(guideline: guidelineOf('g-rust', 'Rust', version: 4, by: robinRef), version: versionOf('g-rust', 4, 'A')));
        await pumpWithGuidelines(
          tester,
          repo,
          GuidelineEditorPage(guideline: rust, fromVersion: 3, initialTitle: 'Rust', initialText: 'A', onSave: (_, _) async => throw const ProtocolException(ErrorCode.versionConflict, 'conflict')),
        );
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'A\nB');
        await tester.pump();
        await tester.tap(find.byKey(const ValueKey('guideline-save')));
        await settleBoard(tester);
        expect(find.text('robin saved version 4 while you were editing.'), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('conflict-compare')));
        await settleBoard(tester);
        expect(find.text('+ B'), findsOneWidget);
      });
    });

    testWidgets('links: grouped by reports_to with n of m and All, a box that would go over 16 000 is off with the reason (B2, B4)', (tester) async {
      final sets = {
        ('forge', 'p1'): setOf([effectiveOf('g-big', 'Big', 14000)]),
        ('bridge', null): setOf([effectiveOf('g-rust', 'Rust', 6120)]),
      };
      final repo = guidelineRepository(sets: sets);
      final bloc = await pumpWithGuidelines(tester, repo, const SizedBox());
      final result = Completer<Set<String>?>();
      final context = tester.element(find.byType(SizedBox).first);
      unawaited(GuidelineLinksPage.open(
        context,
        GuidelineLinksPage(guidelineId: 'g-rust', title: 'Rust', chars: 6120, initial: const {'e:bridge'}, onSave: (before, after) => bloc.setLinks('g-rust', before, after)),
      ).then(result.complete));
      await settleBoard(tester);
      expect(find.text('Who gets “Rust”'), findsOneWidget);
      expect(find.text('Management team · 0 of 2'), findsOneWidget);
      expect(find.text('Reports to Titan · 1 of 2'), findsOneWidget);
      expect(find.text('Reports to Orion · 0 of 1'), findsOneWidget);
      expect(find.textContaining('Pulse (paused)'), findsOneWidget);
      final forgeRow = tester.widget<CheckboxListTile>(find.byKey(const ValueKey('link-e-forge')));
      expect(forgeRow.onChanged, isNull, reason: '14 000 + 6 120 is over 16 000 in Atlas');
      expect(find.text('Forge would get 20 120 characters in Atlas (max 16 000).'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('link-e-titan')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('links-save')));
      await settleBoard(tester);
      expect(await result.future, {'e:bridge', 'e:titan'});
      final w = guidelineWrites(repo).single;
      expect(w['method'], 'guideline.link');
      expect(paramsOf(w)['target'], {'kind': 'employee', 'id': 'titan'});
    });

    testWidgets('Pulse M2: links count a project without open work; a row turns orange or off before Save', (tester) async {
      final sets = {
        ('bridge', null): setOf([effectiveOf('g-own', 'Own rules', 5047), effectiveOf('g-large-a', 'Large rules A', 6968)]),
        ('bridge', 'p1'): setOf([effectiveOf('g-own', 'Own rules', 5047), effectiveOf('g-large-a', 'Large rules A', 6968), effectiveOf('g-p', 'P1 and P2', 2458, level: GuidelineLevel.project)]),
        ('titan', 'p1'): setOf([effectiveOf('g-titan', 'Lead rules', 12000)]),
      };
      final repo = guidelineRepository(sets: sets);
      await pumpWithGuidelines(tester, repo, const SizedBox());
      final context = tester.element(find.byType(SizedBox).first);
      unawaited(GuidelineLinksPage.open(context, GuidelineLinksPage(guidelineId: 'g-large-b', title: 'Large rules B', chars: 1556, initial: const {}, onSave: (b, a) async {})));
      await settleBoard(tester);
      // 14 473 + 1 556 in Atlas, where Bridge has no task: off before Save, not refused after it.
      expect(tester.widget<CheckboxListTile>(find.byKey(const ValueKey('link-e-bridge'))).onChanged, isNull);
      expect(find.text('Bridge would get 16 029 characters in Atlas (max 16 000).'), findsOneWidget);
      // 12 000 + 1 556 in Atlas is 85 %: ticked, the row is orange.
      await tester.tap(find.byKey(const ValueKey('link-e-titan')));
      await tester.pump();
      final line = tester.widget<Text>(find.descendant(of: find.byKey(const ValueKey('link-e-titan')), matching: find.textContaining('12 000 → 13 556')));
      expect(line.style?.color, Tokens.warning);
    });

    testWidgets('links: the server refusing keeps the choice with its reason (ui §5.1)', (tester) async {
      final repo = guidelineRepository();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenThrow(
          const ProtocolException(ErrorCode.guidelineTooLarge, 'x', tooLarge: GuidelineTooLarge(chars: 16100, limit: 16000, employeeId: 'titan')));
      final bloc = await pumpWithGuidelines(tester, repo, const SizedBox());
      final context = tester.element(find.byType(SizedBox).first);
      unawaited(GuidelineLinksPage.open(context, GuidelineLinksPage(guidelineId: 'g-rust', title: 'Rust', chars: 6120, initial: const {}, onSave: (b, a) => bloc.setLinks('g-rust', b, a))));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('link-e-titan')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('links-save')));
      await settleBoard(tester);
      expect(find.text('Titan would get 16 100 characters (max 16 000). Shorten this text or unlink something.'), findsOneWidget);
      expect(tester.widget<CheckboxListTile>(find.byKey(const ValueKey('link-e-titan'))).value, isTrue);
    });

    group('E14: markdown without HTML, external images or auto-loads', () {
      testWidgets('an image is never loaded; its address shows instead', (tester) async {
        await pumpBoardPage(tester, guidelineRepository(), const SingleChildScrollView(child: GuidelineMarkdown('Look ![logo](https://evil.example/x.png?q=secret) here')));
        expect(find.byType(Image), findsNothing);
        expect(find.byKey(const ValueKey('guideline-image-not-loaded')), findsOneWidget);
        expect(find.textContaining('https://evil.example/x.png?q=secret'), findsOneWidget);
      });

      testWidgets('HTML stays text, inline and as a block', (tester) async {
        await pumpBoardPage(tester, guidelineRepository(), const SingleChildScrollView(child: GuidelineMarkdown('<img src="https://evil.example/a.png">\n\nUse <b>bold</b> here.')));
        expect(find.byType(Image), findsNothing);
        expect(find.textContaining('<img src="https://evil.example/a.png">', findRichText: true), findsOneWidget);
        expect(find.textContaining('<b>bold</b>', findRichText: true), findsOneWidget);
      });

      test('no inline HTML and no bare-URL autolinks in the syntax set', () {
        expect(guidelineExtensions.inlineSyntaxes.map((s) => s.runtimeType.toString()), isNot(contains('InlineHtmlSyntax')));
        expect(guidelineExtensions.inlineSyntaxes.map((s) => s.runtimeType.toString()), isNot(contains('AutolinkExtensionSyntax')));
      });

      testWidgets('a link opens only after a question that shows its full address', (tester) async {
        await pumpBoardPage(tester, guidelineRepository(), const SingleChildScrollView(child: GuidelineMarkdown('See [the docs](https://docs.example/rules?ref=agent).', selectable: false)));
        await tester.tap(find.textContaining('the docs', findRichText: true));
        await settleBoard(tester);
        expect(find.text('Open this link?'), findsOneWidget);
        expect(find.text('https://docs.example/rules?ref=agent'), findsOneWidget);
        await tester.tap(find.text('Cancel'));
        await settleBoard(tester);
        expect(find.text('Open this link?'), findsNothing);
      });

      testWidgets('a proposal\'s diff is plain text: an image in it is not rendered', (tester) async {
        final item = GuidelineProposalItem(proposal: proposalOfGuideline('gp1', text: '![x](https://evil.example/p.png)'), current: rust, baseText: '');
        await pumpBoardPage(tester, guidelineRepository(), Scaffold(body: SingleChildScrollView(child: GuidelineProposalCard(item: item, proposer: 'Forge', canDecide: true, onDecide: (_, {note, title, text, links}) {}))), company: teamCompany());
        expect(find.byType(Image), findsNothing);
        expect(find.text('+ ![x](https://evil.example/p.png)'), findsOneWidget);
      });
    });

    group('the proposal card (ui §7.2)', () {
      Future<List<(bool, String?, String?, List<GuidelineTarget>?)>> pumpCard(WidgetTester tester, GuidelineProposalItem item, {bool canDecide = true, Map<(String?, String?), EffectiveGuidelines> sets = const {}}) async {
        final decided = <(bool, String?, String?, List<GuidelineTarget>?)>[];
        await pumpBoardPage(
          tester,
          guidelineRepository(sets: sets),
          Scaffold(
            body: SingleChildScrollView(
              child: GuidelineProposalCard(item: item, proposer: 'Forge', canDecide: canDecide, onDecide: (approve, {note, title, text, links}) => decided.add((approve, note, text, links))),
            ),
          ),
          link: guidelinesLink,
          company: teamCompany(),
        );
        return decided;
      }

      testWidgets('a change: who, from which version, why, evidence, the diff against the base and the counter; Approve', (tester) async {
        final decided = await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1', text: 'A\nC'), current: rust, baseText: 'A\nB'));
        expect(find.text('New guideline version'), findsOneWidget);
        expect(find.text('Proposed by Forge · v3 → 4'), findsOneWidget);
        expect(find.text('Twice this week a review found blocking I/O.'), findsOneWidget);
        expect(find.text('Review found blocking I/O'), findsOneWidget);
        expect(find.text('− B'), findsOneWidget);
        expect(find.text('+ C'), findsOneWidget);
        expect(find.text('3 / 8 000'), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('guideline-approve-gp1')));
        expect(decided.single.$1, isTrue);
      });

      testWidgets('Reject asks for a note for the proposer (GL-14 AC4)', (tester) async {
        final decided = await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1'), current: rust, baseText: ''));
        await tester.tap(find.byKey(const ValueKey('guideline-reject-gp1')));
        await settleBoard(tester);
        expect(find.text('Note for Forge (optional)'), findsOneWidget);
        await tester.enterText(find.byType(TextField).last, 'Too strict');
        await tester.tap(find.widgetWithText(FilledButton, 'Reject'));
        await settleBoard(tester);
        expect(decided.single, (false, 'Too strict', null, null));
      });

      testWidgets('stale: the reason in red and only Reject (ui §7.1)', (tester) async {
        await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1', baseVersion: 2), current: rust, baseText: ''));
        expect(find.byKey(const ValueKey('guideline-proposal-stale-gp1')), findsOneWidget);
        expect(find.textContaining("so it can't be approved"), findsOneWidget);
        expect(find.byKey(const ValueKey('guideline-approve-gp1')), findsNothing);
        expect(find.byKey(const ValueKey('guideline-reject-gp1')), findsOneWidget);
      });

      testWidgets('a new guideline: all +, Applies to with the suggested links, approved with them (P-61)', (tester) async {
        final decided = await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp2', guidelineId: null, baseVersion: null, title: 'Async Rust', suggested: const [GuidelineTargetEmployee(id: 'forge')])));
        expect(find.text('New guideline'), findsOneWidget);
        expect(find.text('Proposed by Forge'), findsOneWidget);
        expect(find.text('Applies to: Forge'), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('guideline-approve-gp2')));
        expect(decided.single.$4!.map(targetKey), ['e:forge']);
      });

      testWidgets('over 8 000 Approve is off; Edit stays to shorten it (GL-13 AC5, ui §7.1)', (tester) async {
        await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1', text: 'x' * 8100), current: rust, baseText: ''));
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('guideline-approve-gp1'))).onPressed, isNull);
        expect(tester.widget<TextButton>(find.byKey(const ValueKey('guideline-edit-gp1'))).onPressed, isNotNull);
      });

      testWidgets('Trace m5: the largest block after approving, orange from 80 %; Approve stays on below 16 000', (tester) async {
        // Rust (6 120 now) is linked to Forge and Bridge; Forge in Atlas has 10 000 more.
        final sets = {
          ('forge', 'p1'): setOf([effectiveOf('g-rust', 'Rust', 6120, version: 3), effectiveOf('g-big', 'Big', 10000, level: GuidelineLevel.project)]),
        };
        await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1', text: 'x' * 5000), current: rust, baseText: ''), sets: sets);
        await settleBoard(tester);
        final fits = tester.widget<Text>(find.byKey(const ValueKey('guideline-proposal-block-gp1')));
        expect(fits.data, 'Largest block after this: Forge in Atlas · 15 000 / 16 000');
        expect(fits.style?.color, Tokens.warning);
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('guideline-approve-gp1'))).onPressed, isNotNull);
      });

      testWidgets('Trace m5: over 16 000 per run Approve is off with who and where; Edit stays to shorten it', (tester) async {
        final sets = {
          ('forge', 'p1'): setOf([effectiveOf('g-rust', 'Rust', 6120, version: 3), effectiveOf('g-big', 'Big', 10000, level: GuidelineLevel.project)]),
        };
        await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1', text: 'x' * 7000), current: rust, baseText: ''), sets: sets);
        await settleBoard(tester);
        expect(find.text('Largest block after this: Forge in Atlas · 17 000 / 16 000'), findsOneWidget);
        expect(find.text('Forge would get 17 000 characters in Atlas (max 16 000). Approve is off; Edit lets you shorten it.'), findsOneWidget);
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('guideline-approve-gp1'))).onPressed, isNull);
        expect(tester.widget<TextButton>(find.byKey(const ValueKey('guideline-edit-gp1'))).onPressed, isNotNull);
      });

      testWidgets('Edit opens the editor with Approve; its text goes with the approval (GL-17 AC2)', (tester) async {
        final decided = await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1', text: 'A'), current: rust, baseText: 'A'));
        await tester.tap(find.byKey(const ValueKey('guideline-edit-gp1')));
        await settleBoard(tester);
        expect(find.widgetWithText(FilledButton, 'Approve'), findsOneWidget);
        await tester.enterText(find.byKey(const ValueKey('guideline-text')), 'A shorter');
        await tester.pump();
        await tester.tap(find.byKey(const ValueKey('guideline-save')));
        await settleBoard(tester);
        expect(decided.single.$1, isTrue);
        expect(decided.single.$3, 'A shorter');
      });

      testWidgets('without the Approve right: no buttons, "A board member decides" (GL-14 AC6)', (tester) async {
        await pumpCard(tester, GuidelineProposalItem(proposal: proposalOfGuideline('gp1'), current: rust, baseText: ''), canDecide: false);
        expect(find.byKey(const ValueKey('guideline-approve-gp1')), findsNothing);
        expect(find.byKey(const ValueKey('guideline-reject-gp1')), findsNothing);
        expect(find.text('A board member decides.'), findsOneWidget);
        expect(canDecideGuidelines(readerLink), isFalse);
        expect(canDecideGuidelines(guidelinesLink), isTrue);
      });
    });

    testWidgets('the run details: "Guidelines: Rust v3, Git & worktrees v4", a tap opens that version (P-59, GL-11 AC3)', (tester) async {
      final repo = guidelineRepository();
      when(() => repo.runGuidelineList('r1')).thenAnswer((_) async => const [
            RunGuideline(chars: 6120, guidelineId: 'g-rust', level: GuidelineLevel.employee, runId: 'r1', title: 'Rust', version: 3),
            RunGuideline(chars: 2050, guidelineId: 'g-git', level: GuidelineLevel.project, runId: 'r1', title: 'Git & worktrees', version: 4),
          ]);
      await pumpBoardPage(tester, repo, const Scaffold(body: RunGuidelinesLine(runId: 'r1')), link: guidelinesLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text('Guidelines: Rust v3, Git & worktrees v4'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('run-guideline-g-git')));
      await settleBoard(tester);
      verify(() => repo.guideline('g-git', version: 4)).called(1);
    });

    testWidgets('the run details on a server without guidelines ask nothing', (tester) async {
      final repo = guidelineRepository();
      await pumpBoardPage(tester, repo, const RunGuidelinesLine(runId: 'r1'));
      await settleBoard(tester);
      verifyNever(() => repo.runGuidelineList(any()));
      expect(find.byKey(const ValueKey('run-guidelines')), findsNothing);
    });

    testWidgets('an employee\'s Guidelines: per run context in block order with why, against 16 000; Manage for board members (GL-05 AC2, GL-07)', (tester) async {
      final sets = {
        ('forge', null): setOf([effectiveOf('g-rust', 'Rust', 6120, version: 3)], employeeId: 'forge'),
        ('forge', 'p1'): setOf([effectiveOf('g-rust', 'Rust', 6120, version: 3), effectiveOf('g-git', 'Git & worktrees', 7000, version: 4, level: GuidelineLevel.project)], employeeId: 'forge', projectId: 'p1'),
      };
      final repo = guidelineRepository(sets: sets);
      await pumpBoardPage(tester, repo, const Scaffold(body: GuidelinesRow(target: GuidelineTargetEmployee(id: 'forge'))), link: guidelinesLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text('1 · 13 120 / 16 000 characters'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('target-guidelines-row')));
      await settleBoard(tester);
      expect(find.text('Rust v3'), findsOneWidget);
      expect(find.text('linked to Forge · 6 120 chars'), findsOneWidget);
      await tester.tap(find.text('Atlas'));
      await settleBoard(tester);
      expect(find.text('from Atlas · 7 000 chars'), findsOneWidget);
      expect(find.text('Close to the limit'), findsOneWidget, reason: '13 120 is over 80 % of 16 000');
      expect(find.byKey(const ValueKey('target-guidelines-manage')), findsOneWidget);
    });

    testWidgets('a project\'s Guidelines row: how many and how much each run gets (GL-06 AC2)', (tester) async {
      final repo = guidelineRepository(sets: {(null, 'p1'): setOf([effectiveOf('g-git', 'Git & worktrees', 3830, level: GuidelineLevel.project)], projectId: 'p1')});
      await pumpBoardPage(tester, repo, const Scaffold(body: GuidelinesRow(target: GuidelineTargetProject(id: 'p1'))), link: guidelinesLink, company: teamCompany());
      await settleBoard(tester);
      expect(find.text('1 · +3 830 characters per run'), findsOneWidget);
    });
  });
}
