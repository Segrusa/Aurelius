import 'package:aurelius_mobile/app/mobile_core.dart';
import 'dart:async';
import 'dart:io';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/secure/secure_surface.dart';
import 'package:aurelius_mobile/data/vault.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/more/more_page.dart';
import 'package:aurelius_mobile/features/vault/reveal_bloc.dart';
import 'package:aurelius_mobile/features/vault/vault_audit_page.dart';
import 'package:aurelius_mobile/features/vault/vault_bloc.dart';
import 'package:aurelius_mobile/features/vault/vault_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Fase 4 on the phone (AUR-MU-04-M1): the vault of the company and of a
// project, "overrides Company", text ↔ url but never to or from secret, Show
// for 30 s and Copy cleared after 60 s only while still ours (E19), the
// audit log and the texts of R2/R3 (E21). Neutral people only (Alex, Robin)
// and fake secrets.

const fakeSecret = 'sk-test-FAKE-0000';
const alexRef = UserRef(id: 'u-1', username: 'alex');
const robinRef = UserRef(id: 'u-2', username: 'robin');
const alexUser = UserView(id: 'u-1', username: 'alex', createdAtMs: 0);
const vaultLink = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: ['vault'],
  boardMembersExist: true,
  user: alexUser,
  claudeLogin: testLogin,
);

VaultItemView itemOf(String id, String name, String env, VaultKind kind, {String? projectId, String? value, bool? overrides, int version = 1, UserRef by = alexRef, String? description, bool? undecryptable}) => VaultItemView(
      createdAtMs: 0,
      createdBy: by,
      description: description,
      envName: env,
      id: id,
      kind: kind,
      name: name,
      overridesCompany: overrides,
      projectId: projectId,
      undecryptable: undecryptable,
      updatedAtMs: 0,
      updatedBy: by,
      value: kind == VaultKind.secret ? null : value,
      version: version,
    );

final githubToken = itemOf('i-gh', 'GitHub token', 'GITHUB_TOKEN', VaultKind.secret);
final stagingUrl = itemOf('i-stg', 'Staging URL', 'STAGING_URL', VaultKind.url, value: 'https://stg.example.com');
final deployNotes = itemOf('i-dn', 'Deploy notes', 'DEPLOY_NOTES', VaultKind.text, value: 'Run make deploy');
final projectToken = itemOf('i-pgh', 'GitHub token (Atlas)', 'GITHUB_TOKEN', VaultKind.secret, projectId: 'p1', overrides: true);
final projectNote = itemOf('i-pn', 'Atlas notes', 'ATLAS_NOTES', VaultKind.text, projectId: 'p1', value: 'Only staging');

VaultList companyList([List<VaultItemView>? items]) => VaultList(items: items ?? [githubToken, stagingUrl, deployNotes], counts: const [VaultScopeCount(count: 3), VaultScopeCount(count: 2, projectId: 'p1')]);
VaultList projectList() => VaultList(projectId: 'p1', items: [projectToken, projectNote], companyItems: [githubToken, stagingUrl, deployNotes], counts: const [VaultScopeCount(count: 3), VaultScopeCount(count: 2, projectId: 'p1')]);

VaultAuditRow auditOf(int id, VaultAuditAction action, VaultActor actor, {String? item = 'GitHub token', String? via, List<String> changed = const []}) =>
    VaultAuditRow(action: action, actor: actor, atMs: 0, id: id, itemId: item == null ? null : 'i-gh', itemName: item, via: via, changedFields: changed);

/// The phone's sensitive clipboard and screen, recorded instead of native.
class FakeSurface implements SecureSurface {
  final List<bool> secure = [];
  final List<(String, Duration)> copies = [];
  final List<String> clears = [];

  /// What the clipboard holds now, and the token of our copy.
  String? clipboard;
  String? ours;
  int _n = 0;

  /// Android in the background: the clipboard cannot be checked.
  bool canCheck = true;

  @override
  Future<void> setSecure(bool on) async => secure.add(on);

  @override
  Future<ClipToken?> copySensitive(String text, Duration expiresIn) async {
    copies.add((text, expiresIn));
    clipboard = text;
    ours = 'clip-${++_n}';
    return ClipToken(ours!);
  }

  @override
  Future<bool> clearIfOurs(ClipToken token) async {
    if (!canCheck) return false;
    clears.add(token.id);
    if (token.id == ours) clipboard = null;
    return true;
  }

  /// Someone copied something else meanwhile.
  void copiedElsewhere(String text) {
    clipboard = text;
    ours = null;
  }
}

MockRepository vaultRepository({VaultList? company, VaultList? project, List<VaultAuditRow> audit = const []}) {
  final repo = boardRepository();
  when(() => repo.linkState).thenReturn(vaultLink);
  when(() => repo.vaultChanges).thenAnswer((_) => const Stream.empty());
  when(() => repo.vault(projectId: null)).thenAnswer((_) async => company ?? companyList());
  when(() => repo.vault(projectId: 'p1')).thenAnswer((_) async => project ?? projectList());
  when(() => repo.vaultAuditLog(projectId: any(named: 'projectId'), all: any(named: 'all'), itemId: any(named: 'itemId'), before: any(named: 'before'), limit: any(named: 'limit')))
      .thenAnswer((_) async => VaultAuditPage(more: false, rows: audit));
  when(() => repo.revealVaultValue(any(), any())).thenAnswer((_) async => fakeSecret);
  return repo;
}

Future<void> settle(WidgetTester tester) async {
  for (var i = 0; i < 6; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

Future<void> openVault(WidgetTester tester, MockRepository repo, {String? projectId}) => pumpBoardPage(tester, repo, VaultPage(projectId: projectId), link: vaultLink);

void main() {
  late FakeSurface surface;

  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(RevealPurpose.show);
    registerFallbackValue(const VaultCreateParams(envName: 'X', kind: VaultKind.text, name: 'x', value: 'x'));
    registerFallbackValue(const VaultUpdateParams(itemId: 'x'));
  });

  setUp(() {
    surface = FakeSurface();
    SensitiveClipboard.instance = SensitiveClipboard(surface);
  });

  tearDown(() => SensitiveClipboard.instance.reset());

  group('the vault without widgets', () {
    test('the key follows the name in UPPER_SNAKE; only A–Z, 0–9 and _ starting with a letter', () {
      expect(envNameFrom('GitHub token'), 'GITHUB_TOKEN');
      expect(envNameFrom('  staging url (eu) '), 'STAGING_URL_EU');
      expect(envNameFrom('2fa backup'), 'FA_BACKUP');
      expect(validEnvName('GITHUB_TOKEN'), isTrue);
      expect(validEnvName('github_token'), isFalse);
      expect(validEnvName('_TOKEN'), isFalse);
      expect(validEnvName('A' * 65), isFalse);
      expect(envNameError('GITHUB_TOKEN', ['GITHUB_TOKEN']), 'Another item in this scope has this key.');
      expect(envNameError('', const []), 'Enter a key.');
    });

    test('a url is a full http(s) address; a secret has at least 8 characters (E13); an empty secret stays when editing', () {
      expect(vaultValueError(VaultKind.url, 'stg.example.com'), 'Enter a full URL, starting with https://');
      expect(vaultValueError(VaultKind.url, 'https://stg.example.com'), isNull);
      expect(vaultValueError(VaultKind.secret, 'sk-1234'), 'A secret has at least 8 characters.');
      expect(vaultValueError(VaultKind.secret, fakeSecret), isNull);
      expect(vaultValueError(VaultKind.secret, '', keep: true), isNull);
      expect(vaultValueError(VaultKind.text, ''), 'Enter a value.');
    });

    test('B7: a new item may be any kind; text and url change into each other; a secret stays a secret and nothing becomes one', () {
      expect(kindChoices(null), [VaultKind.text, VaultKind.url, VaultKind.secret]);
      expect(kindChoices(VaultKind.text), [VaultKind.text, VaultKind.url]);
      expect(kindChoices(VaultKind.url), [VaultKind.text, VaultKind.url]);
      expect(kindChoices(VaultKind.secret), [VaultKind.secret]);
    });

    test('E21: the texts for R2 (MCP servers) and R3 (the key on Linux)', () {
      expect(vaultMcpNotice, 'MCP servers of an employee see the secrets of its runs.');
      expect(vaultKeyNotice, "On Linux, any process of the daemon's user, also an agent through Bash, can read the key.", reason: 'QA2 m7: the same text as the desktop');
    });

    test('audit lines name who did what to which item, never a value (MU4-09 AC2)', () {
      const alex = VaultActorBoardMember(user: alexRef);
      const robin = VaultActorBoardMember(user: robinRef);
      const forge = VaultActorEmployee(employeeId: 'e-forge', name: 'Forge', runId: 'r-1', ticketId: 'k-42');
      expect(auditLine(auditOf(1, VaultAuditAction.reveal, alex), me: 'u-1'), 'You revealed GitHub token');
      expect(auditLine(auditOf(2, VaultAuditAction.copy, robin), me: 'u-1'), 'robin copied GitHub token');
      expect(auditLine(auditOf(3, VaultAuditAction.deliver, forge), me: 'u-1'), 'Forge (employee) got GitHub token as an environment variable');
      expect(auditLine(auditOf(4, VaultAuditAction.edit, robin, item: 'Staging URL', changed: ['env_name', 'kind']), me: 'u-1'), 'robin edited Staging URL (key, type)');
      expect(auditLine(auditOf(5, VaultAuditAction.keyExport, alex, item: null), me: 'u-1'), 'You exported the vault key');
      expect(auditVia(auditOf(3, VaultAuditAction.deliver, forge)), 'A run of a ticket');
      expect(auditVia(auditOf(1, VaultAuditAction.reveal, alex, via: 'iPhone')), 'iPhone');
      final rows = [auditOf(1, VaultAuditAction.reveal, alex), auditOf(3, VaultAuditAction.deliver, forge)];
      expect(filterAudit(rows, who: AuditWho.employees).map((r) => r.id), [3]);
      expect(filterAudit(rows, action: VaultAuditAction.reveal).map((r) => r.id), [1]);
    });

    test('B10: the question names the items only', () {
      expect(secretCheckBody(['GitHub token']), contains('GitHub token'));
      expect(secretCheckBody(['GitHub token']), isNot(contains(fakeSecret)));
    });

    test('QA2 m5: the question names the mark the PC really stores, «secret:KEY»', () {
      expect(secretCheckBody(['GitHub token']), "It holds the value of GitHub token. If you send it anyway, the value is replaced by «secret:KEY», with the item's key, before it is stored.");
      expect(secretCheckBody(['GitHub token']), isNot(contains('‹secret')));
    });

    test('E19: a reveal state prints no value', () {
      const state = RevealState(itemId: 'i-gh', value: fakeSecret, secondsLeft: 30);
      expect(state.toString(), isNot(contains(fakeSecret)));
      expect(const ClipToken('clip-1').toString(), 'ClipToken');
    });

    test('E19: no logging or debug print anywhere in the app (the generated protocol aside)', () {
      final offenders = <String>[];
      for (final f in Directory('lib').listSync(recursive: true).whereType<File>()) {
        if (!f.path.endsWith('.dart') || f.path.contains('data/protocol/')) continue;
        final source = f.readAsStringSync();
        if (RegExp(r'\b(debugPrint|print|developer\.log)\(').hasMatch(source)) offenders.add(f.path);
      }
      expect(offenders, isEmpty);
    });
  });

  group('VaultBloc', () {
    test('reads the company, then a project with the company apart; vault events read it again (MU4-01 AC2, MU4-02 AC1)', () async {
      final events = StreamController<Event>.broadcast();
      final repo = vaultRepository();
      when(() => repo.vaultChanges).thenAnswer((_) => events.stream);
      final bloc = VaultBloc(repo);
      await stateWhere(bloc, (s) => s.list != null);
      expect(bloc.state.items.map((i) => i.name), ['GitHub token', 'Staging URL', 'Deploy notes']);
      expect(bloc.state.companyItems, isEmpty);
      bloc.add(const VaultScopeChanged('p1'));
      await stateWhere(bloc, (s) => s.list?.projectId == 'p1');
      expect(bloc.state.items.map((i) => i.id), ['i-pgh', 'i-pn']);
      expect(bloc.state.companyItems, hasLength(3));
      expect(bloc.state.countOf('p1'), 2);
      events.add(const Event(seq: 2, kind: EventKind.vaultItemCreated, atMs: 0, aggregateId: 'i-new'));
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verify(() => repo.vault(projectId: 'p1')).called(2);
      await bloc.close();
      await events.close();
    });
  });

  group('the vault screen', () {
    testWidgets('More → Vault: company items with key and type; a secret never shows its value; the R2/R3 texts and the key export on the desktop', (tester) async {
      final repo = vaultRepository();
      await openVault(tester, repo);
      expect(find.text('Vault'), findsWidgets);
      expect(find.text('Company · 3'), findsOneWidget);
      expect(find.text('GitHub token'), findsOneWidget);
      expect(find.text('GITHUB_TOKEN · Secret'), findsOneWidget);
      expect(find.text('STAGING_URL · URL'), findsOneWidget);
      expect(find.textContaining(fakeSecret), findsNothing);
      expect(find.text(vaultMcpNotice), findsOneWidget);
      expect(find.text(vaultKeyNotice), findsOneWidget);
      expect(find.text(vaultKeyExportNotice), findsOneWidget);
      expect(find.bySemanticsLabel(RegExp('GitHub token, secret, hidden')), findsOneWidget);
    });

    testWidgets('a project: its items with "overrides Company", and the company items apart (MU4-02 AC1/AC2, B4, P-42)', (tester) async {
      final repo = vaultRepository();
      await openVault(tester, repo, projectId: 'p1');
      expect(find.text('Atlas · 2'), findsOneWidget);
      expect(find.text('Items of Atlas'), findsNothing, reason: 'the header has its count apart');
      expect(find.text('GitHub token (Atlas)'), findsOneWidget);
      expect(find.byKey(const ValueKey('overrides-company')), findsOneWidget);
      await tester.scrollUntilVisible(find.text('Deploy notes'), 200);
      expect(find.textContaining('Company items also available here'), findsOneWidget);
      expect(find.text('Deploy notes'), findsOneWidget);
    });

    testWidgets('the scope sheet switches from the company to a project', (tester) async {
      final repo = vaultRepository();
      await openVault(tester, repo);
      await tester.tap(find.byKey(const ValueKey('vault-scope')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('vault-scope-p1')));
      await settle(tester);
      expect(find.text('Atlas · 2'), findsOneWidget);
      expect(find.byKey(const ValueKey('overrides-company')), findsOneWidget);
    });

    testWidgets('an empty project says who can read its items', (tester) async {
      final repo = vaultRepository(project: VaultList(projectId: 'p1', items: const [], companyItems: [githubToken]));
      await openVault(tester, repo, projectId: 'p1');
      expect(find.text('No items in Atlas yet. Employees of this project can read every item here.'), findsOneWidget);
    });

    testWidgets('More lists the Vault only on a server that has it (ui §1)', (tester) async {
      mobileCoreHidden = const {};
      addTearDown(() => mobileCoreHidden = kHiddenInMobileCore);
      await pumpBoardPage(tester, vaultRepository(), const MorePage(), link: vaultLink);
      expect(find.byKey(const ValueKey('more-vault')), findsOneWidget);
      await pumpBoardPage(tester, vaultRepository(), const MorePage(), link: onlineLink);
      expect(find.byKey(const ValueKey('more-vault')), findsNothing);
    });

    testWidgets('a server without the vault says so', (tester) async {
      final repo = vaultRepository();
      await pumpBoardPage(tester, repo, const VaultPage(), link: onlineLink);
      expect(find.text('No vault on this server yet'), findsOneWidget);
      verifyNever(() => repo.vault(projectId: any(named: 'projectId')));
    });
  });

  group('Show and Copy (B6, E19)', () {
    Future<void> openToken(WidgetTester tester, MockRepository repo) async {
      await openVault(tester, repo);
      await tester.tap(find.text('GitHub token'));
      await settle(tester);
    }

    Finder inDialog(String text) => find.descendant(of: find.byType(AlertDialog), matching: find.text(text));

    /// Show or Copy, then the same button in the confirmation (MU4-10 AC2).
    Future<void> confirmed(WidgetTester tester, String action) async {
      await tester.tap(find.byKey(ValueKey('vault-${action.toLowerCase()}')));
      await settle(tester);
      await tester.tap(inDialog(action));
      await settle(tester);
    }

    testWidgets('Show asks first; Cancel sends no reveal, so no audit row (MU4-10 AC2)', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      await tester.tap(find.byKey(const ValueKey('vault-show')));
      await settle(tester);
      expect(inDialog('Show GitHub token?'), findsOneWidget);
      expect(inDialog('Shows the value for 30 s. This is logged.'), findsOneWidget);
      expect(inDialog('Show'), findsOneWidget);
      await tester.tap(inDialog('Cancel'));
      await settle(tester);
      expect(find.byType(AlertDialog), findsNothing);
      verifyNever(() => repo.revealVaultValue(any(), any()));
      expect(find.text(maskedValue), findsOneWidget);
      expect(surface.secure, isEmpty);
    });

    testWidgets('Copy asks first; Cancel sends no reveal and copies nothing (MU4-10 AC2)', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      await tester.tap(find.byKey(const ValueKey('vault-copy')));
      await settle(tester);
      expect(inDialog('Copy GitHub token?'), findsOneWidget);
      expect(inDialog('Copies the value. The clipboard clears in 60 s. This is logged.'), findsOneWidget);
      expect(inDialog('Copy'), findsOneWidget);
      await tester.tap(inDialog('Cancel'));
      await settle(tester);
      verifyNever(() => repo.revealVaultValue(any(), any()));
      expect(surface.copies, isEmpty);
      expect(find.text('Copied. Clears in 60 s.'), findsNothing);
    });

    testWidgets('confirming sends one reveal with purpose show, then one with purpose copy (MU4-10 AC2)', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      await confirmed(tester, 'Show');
      verify(() => repo.revealVaultValue('i-gh', RevealPurpose.show)).called(1);
      verifyNever(() => repo.revealVaultValue(any(), RevealPurpose.copy));
      expect(find.text(fakeSecret), findsOneWidget);
      await confirmed(tester, 'Copy');
      verify(() => repo.revealVaultValue('i-gh', RevealPurpose.copy)).called(1);
      verifyNever(() => repo.revealVaultValue(any(), any()));
      // The countdown and the clipboard clear run out.
      await tester.pump(clipboardClearAfter);
      await tester.pump();
    });

    testWidgets('once confirmed, it shows the value for 30 s with a countdown on a secure screen; then it is hidden and gone from the state', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      expect(find.text(maskedValue), findsOneWidget);
      expect(find.text('Shows the value for 30 s. This is logged.'), findsOneWidget);
      await confirmed(tester, 'Show');
      await tester.pump();
      await tester.pump();
      verify(() => repo.revealVaultValue('i-gh', RevealPurpose.show)).called(1);
      expect(find.text(fakeSecret), findsOneWidget);
      expect(find.text('Hides in 0:30'), findsOneWidget);
      expect(surface.secure, [true], reason: 'FLAG_SECURE / the covered snapshot while it shows');
      await tester.pump(const Duration(seconds: 10));
      expect(find.text('Hides in 0:20'), findsOneWidget);
      await tester.pump(const Duration(seconds: 20));
      await tester.pump();
      expect(find.text(fakeSecret), findsNothing);
      expect(find.text(maskedValue), findsOneWidget);
      expect(surface.secure, [true, false]);
      final reveal = tester.element(find.byKey(const ValueKey('vault-secret-value'))).read<RevealBloc>();
      expect(reveal.state.value, isNull, reason: 'no value in the state once hidden');
      expect(reveal.state.itemId, isNull);
    });

    testWidgets('Hide, and the app going to the background, hide it at once', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      await confirmed(tester, 'Show');
      await tester.pump();
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('vault-show')));
      await tester.pump();
      expect(find.text(fakeSecret), findsNothing);
      await confirmed(tester, 'Show');
      await tester.pump();
      await tester.pump();
      expect(find.text(fakeSecret), findsOneWidget);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await tester.pump();
      await tester.pump();
      expect(find.text(fakeSecret), findsNothing);
      expect(surface.secure, [true, false, true, false]);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await tester.pump();
    });

    testWidgets('M-m4: Show and Hide are one button that keeps its place; the live region says "Hidden" once it hides', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      String live() => tester.widget<Text>(find.descendant(of: find.byKey(const ValueKey('vault-reveal-live')), matching: find.byType(Text))).data!;
      final button = find.byKey(const ValueKey('vault-show'));
      expect(tester.widget<Semantics>(find.byKey(const ValueKey('vault-reveal-live'))).properties.liveRegion, isTrue);
      expect(live(), isEmpty, reason: 'nothing to announce before a Show');
      expect(find.byKey(const ValueKey('vault-show-hint')), findsOneWidget);
      final before = tester.element(button);
      await confirmed(tester, 'Show');
      await tester.pump();
      expect(find.descendant(of: button, matching: find.text('Hide')), findsOneWidget);
      expect(tester.element(button), same(before), reason: 'the same element, so the focus stays');
      expect(live(), 'Shown for 30 seconds. This is logged.');
      expect(find.byKey(const ValueKey('vault-show-hint')), findsNothing);
      await tester.tap(button);
      await tester.pump();
      expect(find.descendant(of: button, matching: find.text('Show')), findsOneWidget);
      expect(tester.element(button), same(before));
      expect(live(), 'Hidden');
      expect(find.text(showDetail()), findsOneWidget, reason: 'the hint is back, outside the live region');
    });

    testWidgets('M-m2: a Show reads the history once more, not on every tick of the countdown', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      int audits() => verify(() => repo.vaultAuditLog(projectId: any(named: 'projectId'), all: any(named: 'all'), itemId: 'i-gh', before: any(named: 'before'), limit: any(named: 'limit'))).callCount;
      expect(audits(), 1, reason: 'the page read it once');
      await confirmed(tester, 'Show');
      await tester.pump(const Duration(seconds: 10));
      expect(audits(), 1, reason: 'once after the reveal');
      await tester.pump(const Duration(seconds: 25));
      expect(find.text(fakeSecret), findsNothing);
      // The countdown and the hide read nothing.
      verifyNever(() => repo.vaultAuditLog(projectId: any(named: 'projectId'), all: any(named: 'all'), itemId: any(named: 'itemId'), before: any(named: 'before'), limit: any(named: 'limit')));
    });

    testWidgets('M-m7: Edit hides a shown value before the editor covers the screen', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      await confirmed(tester, 'Show');
      await tester.pump();
      expect(find.text(fakeSecret), findsOneWidget);
      final reveal = tester.element(find.byKey(const ValueKey('vault-secret-value'))).read<RevealBloc>();
      await tester.tap(find.byTooltip('Edit'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('vault-save')), findsOneWidget, reason: 'the editor is open');
      expect(reveal.state.value, isNull);
      expect(find.text(fakeSecret, skipOffstage: false), findsNothing);
      expect(surface.secure, [true, false]);
    });

    testWidgets('Copy puts it on the clipboard as sensitive without showing it, and clears it after 60 s', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      await confirmed(tester, 'Copy');
      await tester.pump();
      await tester.pump();
      verify(() => repo.revealVaultValue('i-gh', RevealPurpose.copy)).called(1);
      expect(surface.copies.single, (fakeSecret, const Duration(seconds: 60)));
      expect(find.text(fakeSecret), findsNothing);
      expect(find.text('Copied. Clears in 60 s.'), findsOneWidget);
      expect(surface.secure, isEmpty, reason: 'nothing shows');
      await tester.pump(const Duration(seconds: 59));
      expect(surface.clipboard, fakeSecret);
      await tester.pump(const Duration(seconds: 1));
      expect(surface.clipboard, isNull);
      expect(SensitiveClipboard.instance.pending, isFalse);
    });

    testWidgets('the clear leaves alone what was copied since, also after leaving the screen', (tester) async {
      final repo = vaultRepository();
      await openToken(tester, repo);
      await confirmed(tester, 'Copy');
      await tester.pump();
      await tester.pump();
      await tester.pageBack();
      await settle(tester);
      surface.copiedElsewhere('a link Robin sent');
      await tester.pump(const Duration(seconds: 60));
      expect(surface.clears, ['clip-1']);
      expect(surface.clipboard, 'a link Robin sent');
    });

    test('M-m3: a second Show that fails ends the secure screen; a timeout leaves it not busy', () async {
      final repo = vaultRepository();
      var calls = 0;
      when(() => repo.revealVaultValue(any(), any())).thenAnswer((_) async {
        calls++;
        if (calls == 2) throw const ProtocolException(ErrorCode.validation, 'refused');
        if (calls == 3) throw TimeoutException('slow');
        return fakeSecret;
      });
      final bloc = RevealBloc(repo);
      bloc.add(const RevealShown('i-gh'));
      await stateWhere(bloc, (s) => s.shown);
      expect(surface.secure, [true]);
      bloc.add(const RevealShown('i-gh'));
      await stateWhere(bloc, (s) => !s.busy && s.notice != null);
      expect(bloc.state.shown, isFalse);
      expect(surface.secure, [true, false], reason: 'no FLAG_SECURE without a value');
      bloc.add(const RevealNoticeShown());
      await stateWhere(bloc, (s) => s.notice == null);
      bloc.add(const RevealShown('i-gh'));
      await stateWhere(bloc, (s) => !s.busy && s.notice != null);
      expect(bloc.state.notice, couldNotShow);
      expect(surface.secure, [true, false]);
      await bloc.close();
      expect(surface.secure, [true, false], reason: 'close() has nothing to undo');
    });

    testWidgets('an undecryptable value says so instead of failing (MU4-04 AC3)', (tester) async {
      final repo = vaultRepository();
      when(() => repo.revealVaultValue(any(), any())).thenThrow(const ProtocolException(ErrorCode.vaultUndecryptable, 'undecryptable'));
      await openToken(tester, repo);
      await confirmed(tester, 'Show');
      await tester.pump();
      await tester.pump();
      expect(find.text('Cannot be decrypted: the vault key on the PC does not fit.'), findsOneWidget);
      expect(surface.secure, isEmpty);
    });
  });

  group('SensitiveClipboard', () {
    testWidgets('Android in the background cannot check the clipboard: the clear runs once the app is back', (tester) async {
      surface.canCheck = false;
      await SensitiveClipboard.instance.copy(fakeSecret, const Duration(seconds: 60));
      await tester.pump(const Duration(seconds: 61));
      expect(surface.clipboard, fakeSecret);
      expect(SensitiveClipboard.instance.pending, isTrue);
      surface.canCheck = true;
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await tester.pump();
      expect(surface.clipboard, isNull);
      expect(SensitiveClipboard.instance.pending, isFalse);
    });

    testWidgets('a new copy replaces the pending clear: only the last copy is ours', (tester) async {
      await SensitiveClipboard.instance.copy(fakeSecret, const Duration(seconds: 60));
      await tester.pump(const Duration(seconds: 30));
      await SensitiveClipboard.instance.copy('sk-test-FAKE-1111', const Duration(seconds: 60));
      await tester.pump(const Duration(seconds: 30));
      expect(surface.clears, isEmpty, reason: 'the first timer was replaced');
      await tester.pump(const Duration(seconds: 30));
      expect(surface.clears, ['clip-2']);
      expect(surface.clipboard, isNull);
    });
  });

  group('ChannelSecureSurface', () {
    TestWidgetsFlutterBinding.ensureInitialized();

    test('on a phone: the flags and the clipboard go to the native side, which keeps the value to itself', () async {
      final calls = <MethodCall>[];
      const channel = MethodChannel('test/secure');
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(channel, (call) async {
        calls.add(call);
        return switch (call.method) {
          'copySensitive' => 'clip-7',
          'clearIfOurs' => true,
          _ => null,
        };
      });
      addTearDown(() => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(channel, null));
      final surface = ChannelSecureSurface(channel: channel, native: true);
      await surface.setSecure(true);
      final token = await surface.copySensitive(fakeSecret, const Duration(seconds: 60));
      expect(token?.id, 'clip-7');
      expect(await surface.clearIfOurs(token!), isTrue);
      expect(calls.map((c) => c.method), ['setSecure', 'copySensitive', 'clearIfOurs']);
      expect(calls[0].arguments, {'on': true});
      expect(calls[1].arguments, {'text': fakeSecret, 'expiresInMs': 60000});
      expect(calls[2].arguments, {'id': 'clip-7'});
    });

    test('elsewhere: the plain clipboard, cleared only while it still holds our copy', () async {
      String? clip;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
        if (call.method == 'Clipboard.setData') clip = (call.arguments as Map)['text'] as String?;
        if (call.method == 'Clipboard.getData') return {'text': clip};
        return null;
      });
      addTearDown(() => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));
      final surface = ChannelSecureSurface(native: false);
      final token = (await surface.copySensitive(fakeSecret, const Duration(seconds: 60)))!;
      expect(token.id, isNot(contains(fakeSecret)));
      clip = 'something Alex copied';
      await surface.clearIfOurs(token);
      expect(clip, 'something Alex copied');
      final again = (await surface.copySensitive(fakeSecret, const Duration(seconds: 60)))!;
      await surface.clearIfOurs(again);
      expect(clip, '');
    });
  });

  group('the editor', () {
    Future<void> openNew(WidgetTester tester, MockRepository repo) async {
      await openVault(tester, repo);
      await tester.tap(find.byTooltip('New item'));
      await tester.pumpAndSettle();
    }

    testWidgets('a new secret: masked input without suggestions, the key from the name, saved in the scope (MU4-01 AC2)', (tester) async {
      final repo = vaultRepository();
      when(() => repo.createVaultItem(any())).thenAnswer((_) async => githubToken);
      await openNew(tester, repo);
      await tester.tap(find.text('Secret'));
      await tester.pump();
      await tester.enterText(find.byKey(const ValueKey('vault-name')), 'Deploy key');
      await tester.pump();
      expect(find.text('DEPLOY_KEY'), findsOneWidget, reason: 'the key follows the name');
      final value = tester.widget<TextField>(find.byKey(const ValueKey('vault-value')));
      expect((value.obscureText, value.enableSuggestions, value.autocorrect), (true, false, false));
      await tester.enterText(find.byKey(const ValueKey('vault-value')), 'short');
      await tester.tap(find.byKey(const ValueKey('vault-save')));
      await tester.pump();
      expect(find.text('A secret has at least 8 characters.'), findsOneWidget);
      verifyNever(() => repo.createVaultItem(any()));
      await tester.enterText(find.byKey(const ValueKey('vault-value')), fakeSecret);
      await tester.tap(find.byKey(const ValueKey('vault-save')));
      await settle(tester);
      final params = verify(() => repo.createVaultItem(captureAny())).captured.single as VaultCreateParams;
      expect((params.kind, params.name, params.envName, params.value, params.projectId), (VaultKind.secret, 'Deploy key', 'DEPLOY_KEY', fakeSecret, null));
      expect(find.byKey(const ValueKey('vault-save')), findsNothing, reason: 'the editor closed');
    });

    testWidgets('m9 (M-m1): text and url say they are not secret and not redacted; a secret does not', (tester) async {
      final repo = vaultRepository();
      await openNew(tester, repo);
      expect(find.text('Not secret: shown to everyone and not redacted. Use Secret for keys and tokens.'), findsOneWidget, reason: 'text, the default');
      await tester.tap(find.text('URL'));
      await tester.pump();
      expect(find.text(vaultNotSecretHint), findsOneWidget);
      await tester.tap(find.text('Secret'));
      await tester.pump();
      expect(find.text(vaultNotSecretHint), findsNothing);
    });

    testWidgets('M-m6: a new item typed as a secret and then set to Text or URL loses what was typed', (tester) async {
      final repo = vaultRepository();
      await openNew(tester, repo);
      await tester.tap(find.text('Secret'));
      await tester.pump();
      await tester.enterText(find.byKey(const ValueKey('vault-value')), fakeSecret);
      await tester.pump();
      await tester.tap(find.text('Text'));
      await tester.pump();
      expect(tester.widget<TextField>(find.byKey(const ValueKey('vault-value'))).controller!.text, isEmpty);
      expect(find.text(fakeSecret), findsNothing);
      // Text to URL keeps what was typed: neither is secret.
      await tester.enterText(find.byKey(const ValueKey('vault-value')), 'https://stg.example.com');
      await tester.tap(find.text('URL'));
      await tester.pump();
      expect(tester.widget<TextField>(find.byKey(const ValueKey('vault-value'))).controller!.text, 'https://stg.example.com');
    });

    testWidgets('a key the scope has already, and a url without https://, are refused before sending (MU4-01 AC3)', (tester) async {
      final repo = vaultRepository();
      await openNew(tester, repo);
      await tester.tap(find.text('URL'));
      await tester.pump();
      await tester.enterText(find.byKey(const ValueKey('vault-name')), 'Staging url');
      await tester.enterText(find.byKey(const ValueKey('vault-value')), 'stg.example.com');
      await tester.tap(find.byKey(const ValueKey('vault-save')));
      await tester.pump();
      expect(find.text('Another item in this scope has this key.'), findsOneWidget);
      expect(find.text('Enter a full URL, starting with https://'), findsOneWidget);
      verifyNever(() => repo.createVaultItem(any()));
    });

    testWidgets('a url item may become text, never a secret (B7)', (tester) async {
      final repo = vaultRepository();
      when(() => repo.updateVaultItem(any(), version: any(named: 'version'))).thenAnswer((_) async => stagingUrl);
      await openVault(tester, repo);
      await tester.tap(find.text('Staging URL'));
      await settle(tester);
      await tester.tap(find.byTooltip('Edit'));
      await tester.pumpAndSettle();
      expect(find.descendant(of: find.byKey(const ValueKey('vault-kind')), matching: find.text('Secret')), findsNothing);
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('vault-kind')), matching: find.text('Text')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('vault-save')));
      await settle(tester);
      final params = verify(() => repo.updateVaultItem(captureAny(), version: 1)).captured.single as VaultUpdateParams;
      expect(params.toJson(), {'item_id': 'i-stg', 'kind': 'text'}, reason: 'only what changed');
    });

    testWidgets('a secret: the type is locked, the value is never filled in, and left empty it stays (B7, ui §4)', (tester) async {
      final repo = vaultRepository();
      when(() => repo.updateVaultItem(any(), version: any(named: 'version'))).thenAnswer((_) async => githubToken);
      await openVault(tester, repo);
      await tester.tap(find.text('GitHub token'));
      await settle(tester);
      await tester.tap(find.byTooltip('Edit'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('vault-kind-locked')), findsOneWidget);
      expect(find.byKey(const ValueKey('vault-kind')), findsNothing);
      expect(find.text('Leave empty to keep the current secret'), findsOneWidget);
      verifyNever(() => repo.revealVaultValue(any(), any()));
      await tester.enterText(find.byKey(const ValueKey('vault-env')), 'GH_TOKEN');
      await tester.pump();
      expect(find.text('Runs that use GITHUB_TOKEN will no longer find it.'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('vault-save')));
      await settle(tester);
      final params = verify(() => repo.updateVaultItem(captureAny(), version: 1)).captured.single as VaultUpdateParams;
      expect(params.toJson(), {'item_id': 'i-gh', 'env_name': 'GH_TOKEN'}, reason: 'no value: the secret stays');
    });

    testWidgets('the server refusing a change of kind (vault_kind_locked) keeps the editor open with the reason', (tester) async {
      final repo = vaultRepository();
      when(() => repo.updateVaultItem(any(), version: any(named: 'version'))).thenThrow(const ProtocolException(ErrorCode.vaultKindLocked, 'kind locked'));
      await openVault(tester, repo);
      await tester.tap(find.text('Deploy notes'));
      await settle(tester);
      await tester.tap(find.byTooltip('Edit'));
      await tester.pumpAndSettle();
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('vault-kind')), matching: find.text('URL')));
      await tester.enterText(find.byKey(const ValueKey('vault-value')), 'https://docs.example.com');
      await tester.tap(find.byKey(const ValueKey('vault-save')));
      await settle(tester);
      expect(find.text('A secret stays a secret, and nothing becomes one. Make a new item instead.'), findsOneWidget);
    });

    testWidgets('Delete asks with the name and what runs lose, then archives (MU4-11 AC2)', (tester) async {
      final repo = vaultRepository();
      when(() => repo.archiveVaultItem(any())).thenAnswer((_) async => githubToken);
      await openVault(tester, repo);
      await tester.tap(find.text('GitHub token'));
      await settle(tester);
      await tester.tap(find.byTooltip('Delete'));
      await tester.pumpAndSettle();
      expect(find.text('Delete GitHub token?'), findsOneWidget);
      expect(find.textContaining('new runs no longer get GITHUB_TOKEN'), findsOneWidget);
      await tester.tap(find.text('Delete').last);
      await settle(tester);
      verify(() => repo.archiveVaultItem('i-gh')).called(1);
    });
  });

  group('the audit log (MU4-09)', () {
    final rows = [
      auditOf(9, VaultAuditAction.reveal, const VaultActorBoardMember(user: alexRef), via: 'iPhone'),
      auditOf(8, VaultAuditAction.deliver, const VaultActorEmployee(employeeId: 'e-forge', name: 'Forge', runId: 'r-1')),
      auditOf(7, VaultAuditAction.replace, const VaultActorBoardMember(user: robinRef), item: 'Sentry DSN', via: 'Desktop · dev-laptop'),
    ];

    testWidgets('lines without values, filtered on the phone, older ones on request', (tester) async {
      final repo = vaultRepository();
      var calls = 0;
      when(() => repo.vaultAuditLog(projectId: any(named: 'projectId'), all: any(named: 'all'), itemId: any(named: 'itemId'), before: any(named: 'before'), limit: any(named: 'limit')))
          .thenAnswer((_) async => calls++ == 0 ? VaultAuditPage(more: true, rows: rows) : VaultAuditPage(more: false, rows: [auditOf(3, VaultAuditAction.create, const VaultActorBoardMember(user: robinRef), item: 'Staging URL')]));
      await pumpBoardPage(tester, repo, const VaultAuditLogPage(scopeName: 'Company'), link: vaultLink);
      expect(find.text('You revealed GitHub token'), findsOneWidget);
      expect(find.text('just now · iPhone'), findsNothing, reason: 'the time of row 0 ms is long ago');
      expect(find.textContaining('iPhone'), findsOneWidget);
      expect(find.text('Forge (employee) got GitHub token as an environment variable'), findsOneWidget);
      expect(find.text('robin replaced Sentry DSN'), findsOneWidget);
      await tester.tap(find.byTooltip('Filter'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('audit-who-employees')));
      await tester.pumpAndSettle();
      await tester.tapAt(const Offset(10, 10));
      await tester.pumpAndSettle();
      expect(find.text('You revealed GitHub token'), findsNothing);
      expect(find.text('Forge (employee) got GitHub token as an environment variable'), findsOneWidget);
      await tester.scrollUntilVisible(find.byKey(const ValueKey('audit-older')), 200);
      await tester.tap(find.byKey(const ValueKey('audit-older')));
      await settle(tester);
      verify(() => repo.vaultAuditLog(projectId: null, all: false, itemId: null, before: 7, limit: 50)).called(1);
    });

    testWidgets('an item page shows its own history', (tester) async {
      final repo = vaultRepository(audit: rows.take(1).toList());
      await openVault(tester, repo);
      await tester.tap(find.text('GitHub token'));
      await settle(tester);
      await tester.scrollUntilVisible(find.text('You revealed GitHub token'), 200);
      verify(() => repo.vaultAuditLog(itemId: 'i-gh', limit: 20)).called(greaterThanOrEqualTo(1));
    });
  });
}
