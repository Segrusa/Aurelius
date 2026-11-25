import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart' show LinkState;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/links.dart';
import 'package:aurelius_mobile/data/markdown_paths.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/hosts/hosts_bloc.dart';
import 'package:aurelius_mobile/features/links/link_actions.dart';
import 'package:aurelius_mobile/features/markdown/links.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/gestures.dart' show kLongPressTimeout;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/tickets.dart';

// A5a (US-A5 in the app, freeze §2, ui-ontwerp §6): the strict form, Copy link,
// and what opening one does; hidden on a server without `message_locate`.

const _host = '01a19bd5-6957-73a6-a074-d98f1c80f0b2';
const _otherHost = '01a19bd5-6957-73a6-a074-d98f1c80f0b3';
const _ticket = '01a19625-8790-74ca-807b-758adf34b196';
const _message = '01a19625-8790-74ca-807b-758adf34b197';

const _ticketLink = 'aurelius://$_host/ticket/$_ticket';
const _messageLink = 'aurelius://$_host/message/$_message';

class _MockHostsBloc extends MockBloc<HostsEvent, HostsState> implements HostsBloc {}

/// A page with a button that opens [raw], under a HostsBloc that knows this
/// server and, when [known], also "Thuis".
Future<HostsBloc> pumpHosted(WidgetTester tester, MockRepository repo, String raw, {bool known = false}) async {
  final hosts = _MockHostsBloc();
  whenListen(
    hosts,
    const Stream<HostsState>.empty(),
    initialState: HostsState(loading: false, activeId: _host, hosts: [
      const Host(hostId: _host, name: 'Segrusa', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0),
      if (known) const Host(hostId: _otherHost, name: 'Thuis', url: 'http://y', clientId: 'c', environment: 'dev', pairedAtMs: 0),
    ]),
  );
  await pumpBoardPage(
    tester,
    repo,
    BlocProvider<HostsBloc>.value(value: hosts, child: Builder(builder: (context) => Scaffold(body: TextButton(onPressed: () => openAureliusLink(context, raw), child: const Text('open'))))),
    link: phaseALink,
  );
  await tester.tap(find.text('open'));
  await settleBoard(tester);
  return hosts;
}

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(const HostsChanged());
  });

  group('parseAureliusLink', () {
    test('a ticket link gives the server and the ticket', () {
      final link = parseAureliusLink(_ticketLink);
      expect(link, isA<TicketLink>());
      expect(link!.hostId, _host);
      expect(link.id, _ticket);
    });

    test('a message link gives the server and the message', () {
      final link = parseAureliusLink(_messageLink);
      expect(link, isA<MessageLink>());
      expect(link!.id, _message);
    });

    test('what the copy functions write parses back', () {
      expect(parseAureliusLink(ticketLinkOf(_host, _ticket))?.id, _ticket);
      expect(parseAureliusLink(messageLinkOf(_host, _message))?.id, _message);
    });

    for (final (why, raw) in [
      ('empty', ''),
      ('another scheme', 'https://$_host/ticket/$_ticket'),
      ('an upper-case scheme', 'AURELIUS://$_host/ticket/$_ticket'),
      ('no host', 'aurelius:///ticket/$_ticket'),
      ('a host that is no id', 'aurelius://segrusa.local/ticket/$_ticket'),
      ('a port', 'aurelius://$_host:443/ticket/$_ticket'),
      ('a user', 'aurelius://me@$_host/ticket/$_ticket'),
      ('an unknown kind', 'aurelius://$_host/channel/$_ticket'),
      ('no id', 'aurelius://$_host/ticket/'),
      ('a ticket number instead of an id', 'aurelius://$_host/ticket/42'),
      ('an id that is no id', 'aurelius://$_host/ticket/$_ticket-x'),
      ('two kinds', 'aurelius://$_host/ticket/$_ticket/message/$_message'),
      ('a trailing slash', '$_ticketLink/'),
      ('a query', '$_ticketLink?token=abc'),
      ('a fragment', '$_ticketLink#top'),
      ('a path out', 'aurelius://$_host/ticket/../$_ticket'),
      ('text around it', ' $_ticketLink'),
      ('a second line', '$_ticketLink\n'),
    ]) {
      test('refuses $why', () => expect(parseAureliusLink(raw), isNull));
    }
  });

  group('in a text', () {
    String label(AureliusLink link) => link is TicketLink ? '#42' : 'Message';

    test('a link written out reads #42, without the punctuation after it', () {
      expect(labelAureliusLinks('See $_ticketLink, please.', label), 'See [#42]($_ticketLink), please.');
    });

    test('in inline or fenced code it stays code', () {
      expect(labelAureliusLinks('`$_ticketLink`', label), '`$_ticketLink`');
      expect(labelAureliusLinks('```\n$_ticketLink\n```', label), '```\n$_ticketLink\n```');
    });

    test('an invalid one stays plain text', () {
      expect(labelAureliusLinks('aurelius://$_host/ticket/42', label), 'aurelius://$_host/ticket/42');
    });

    test('one already in a Markdown link is left as it is', () {
      expect(labelAureliusLinks('[here]($_ticketLink)', label), '[here]($_ticketLink)');
    });

    test('plain text finds it among the other links, in order', () {
      final links = plainTextLinks('a https://x.io b $_messageLink');
      expect(links.map((l) => l.target).toList(), ['https://x.io', _messageLink]);
    });
  });

  group('Copy link', () {
    String? clipboard;

    setUp(() {
      clipboard = null;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
        if (call.method == 'Clipboard.setData') clipboard = (call.arguments as Map)['text'] as String?;
        return null;
      });
    });

    tearDown(() => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));

    MockRepository linkedRepo(TicketView ticket, {LinkState link = phaseALink}) {
      final repo = boardRepository(board: boardOf([ticket]), detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []), link: link);
      when(() => repo.host).thenReturn(const Host(hostId: _host, name: 'Segrusa', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
      when(() => repo.followers(any())).thenAnswer((_) async => const TicketFollowersView(followers: []));
      return repo;
    }

    Future<void> openMenu(WidgetTester tester, String id) async {
      final gesture = await tester.startGesture(tester.getCenter(find.byKey(ValueKey('ticket-$id'))));
      await tester.pump(kLongPressTimeout + const Duration(milliseconds: 50));
      await gesture.up();
      await settleBoard(tester);
    }

    testWidgets('the card\'s menu copies the link beside Copy #42; the toast says #42', (tester) async {
      final repo = linkedRepo(ticketOf(id: _ticket, number: 42, column: ColumnKind.doing));
      await pumpBoardPage(tester, repo, BlocProvider(create: (_) => BoardBloc(repo), child: const Scaffold(body: BoardPage())), link: phaseALink);
      await openMenu(tester, _ticket);
      expect(find.byKey(const ValueKey('card-menu-item-Copy #42')), findsOneWidget);
      await tester.ensureVisible(find.byKey(const ValueKey('card-menu-item-Copy link')));
      await tester.tap(find.byKey(const ValueKey('card-menu-item-Copy link')));
      await settleBoard(tester);
      expect(clipboard, _ticketLink);
      expect(find.text('Link to #42 copied.'), findsOneWidget);
    });

    testWidgets('an older server\'s menu has no Copy link', (tester) async {
      final repo = linkedRepo(ticketOf(id: _ticket, number: 42, column: ColumnKind.doing), link: onlineLink);
      await pumpBoardPage(tester, repo, BlocProvider(create: (_) => BoardBloc(repo), child: const Scaffold(body: BoardPage())));
      await openMenu(tester, _ticket);
      expect(find.byKey(const ValueKey('card-menu-item-Copy link')), findsNothing);
    });

    testWidgets('the ticket page\'s ⋯ has it, on an older server not', (tester) async {
      final repo = linkedRepo(ticketOf(id: _ticket, number: 42));
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: _ticket), link: phaseALink);
      await tester.tap(find.byTooltip('Ticket actions'));
      await tester.pump(const Duration(milliseconds: 400));
      await tester.tap(find.text('Copy link'));
      await tester.pump(const Duration(milliseconds: 400));
      expect(clipboard, _ticketLink);
    });

    testWidgets('…and without message_locate the ⋯ has none', (tester) async {
      final repo = linkedRepo(ticketOf(id: _ticket, number: 42), link: onlineLink);
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: _ticket));
      await tester.tap(find.byTooltip('Ticket actions'));
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.text('Copy link'), findsNothing);
      expect(find.text('Copy #42'), findsOneWidget);
    });
  });

  group('a link in a message', () {
    setUp(forgetLinkLabels);

    MockRepository repoFor({LinkState link = phaseALink}) {
      final repo = boardRepository(link: link);
      when(() => repo.host).thenReturn(const Host(hostId: _host, name: 'Segrusa', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
      when(() => repo.ticket(any())).thenAnswer((_) async => TicketDetail(asOfSeq: 1, ticket: ticketOf(id: _ticket, number: 42), tasks: const []));
      return repo;
    }

    Widget text(String data) => Scaffold(body: MessageMarkdown(data: data, scope: null));

    Future<GoRouter> tapShown(WidgetTester tester, MockRepository repo, String data, String shown, {LinkState link = phaseALink}) async {
      final router = await pumpBoardPage(tester, repo, text(data), link: link);
      await settleBoard(tester);
      await tester.tapOnText(find.textRange.ofSubstring(shown));
      await settleBoard(tester);
      return router;
    }

    testWidgets('reads #42 and opens the ticket; the id stays out of sight', (tester) async {
      final repo = repoFor();
      final router = await tapShown(tester, repo, 'See $_ticketLink now', '#42');
      expect(router.state.uri.path, '/ticket/$_ticket');
    });

    testWidgets('a ticket that cannot be read reads Unavailable link', (tester) async {
      final repo = repoFor();
      when(() => repo.ticket(any())).thenThrow(const ProtocolException(ErrorCode.notFound, 'gone'));
      await pumpBoardPage(tester, repo, text('See $_ticketLink'), link: phaseALink);
      await settleBoard(tester);
      expect(find.textContaining('Unavailable link', findRichText: true), findsOneWidget);
    });

    testWidgets('a message in a ticket opens its discussion on that message', (tester) async {
      final repo = repoFor();
      when(() => repo.messageLocation(_message)).thenAnswer((_) async => const MessageLocation(conversationId: 'c1', messageId: _message, ticketId: _ticket, ticketNumber: 42));
      final router = await tapShown(tester, repo, 'See $_messageLink', 'Message in #42');
      expect(router.state.uri.toString(), '/ticket/$_ticket?tab=discussion&message=$_message');
    });

    testWidgets("a reply in a ticket's thread opens the ticket's discussion at it (ticket #71: no ticket channels)", (tester) async {
      final repo = repoFor();
      when(() => repo.messageLocation(_message)).thenAnswer((_) async => const MessageLocation(conversationId: 'c1', messageId: _message, ticketId: _ticket, ticketNumber: 42, threadRoot: 'root'));
      final router = await tapShown(tester, repo, 'See $_messageLink', 'Message in #42');
      expect(router.state.uri.toString(), '/ticket/$_ticket?tab=discussion&message=$_message');
    });

    testWidgets('a reply in a thread of a channel that is no ticket (chat is gone) opens the Inbox', (tester) async {
      final repo = repoFor();
      when(() => repo.messageLocation(_message)).thenAnswer((_) async => const MessageLocation(conversationId: 'c1', messageId: _message, threadRoot: 'root'));
      final router = await tapShown(tester, repo, 'See $_messageLink', 'Message');
      expect(router.state.uri.toString(), '/inbox');
    });

    testWidgets('a message in General or a project channel (chat is gone) opens the Inbox, no channel screen', (tester) async {
      final repo = repoFor();
      when(() => repo.messageLocation(_message)).thenAnswer((_) async => const MessageLocation(conversationId: 'c1', messageId: _message));
      final router = await tapShown(tester, repo, 'See $_messageLink', 'Message');
      expect(router.state.uri.toString(), '/inbox');
    });

    testWidgets('a message that is gone says so and goes nowhere', (tester) async {
      final repo = repoFor();
      when(() => repo.messageLocation(_message)).thenThrow(const ProtocolException(ErrorCode.notFound, 'not found'));
      final router = await pumpBoardPage(tester, repo, Builder(builder: (context) => Scaffold(body: TextButton(onPressed: () => openAureliusLink(context, _messageLink), child: const Text('open')))), link: phaseALink);
      await tester.tap(find.text('open'));
      await settleBoard(tester);
      expect(find.text('That message no longer exists.'), findsOneWidget);
      expect(router.state.uri.path, '/');
    });

    Future<void> openFrom(WidgetTester tester, MockRepository repo, String raw, {LinkState link = phaseALink}) async {
      await pumpBoardPage(tester, repo, Builder(builder: (context) => Scaffold(body: TextButton(onPressed: () => openAureliusLink(context, raw), child: const Text('open')))), link: link);
      await tester.tap(find.text('open'));
      await settleBoard(tester);
    }

    testWidgets('an invalid link says so and navigates nowhere', (tester) async {
      final repo = repoFor();
      await openFrom(tester, repo, 'aurelius://$_host/ticket/42');
      expect(find.text(invalidLinkText), findsOneWidget);
      verifyNever(() => repo.messageLocation(any()));
    });

    testWidgets('an explicit Markdown link that is invalid never goes to the browser', (tester) async {
      final repo = repoFor();
      await pumpBoardPage(tester, repo, text('[here](aurelius://$_host/ticket/42)'), link: phaseALink);
      await tester.tapOnText(find.textRange.ofSubstring('here'));
      await settleBoard(tester);
      expect(find.text(invalidLinkText), findsOneWidget);
    });

    testWidgets('an older server reads a neutral word and stays where it is (no update screen, OPR-2)', (tester) async {
      final repo = repoFor(link: onlineLink);
      final router = await tapShown(tester, repo, 'See $_ticketLink', 'Aurelius link', link: onlineLink);
      final before = router.state.uri.toString();
      expect(find.text("This server can't open links yet. Update the server."), findsOneWidget);
      expect(find.text('Updates'), findsNothing);
      verifyNever(() => repo.ticket(any()));
      expect(router.state.uri.toString(), before);
    });

    testWidgets('a link for a server this phone does not know points at the paired servers', (tester) async {
      final repo = repoFor();
      await pumpHosted(tester, repo, 'aurelius://$_otherHost/ticket/$_ticket');
      expect(find.text("This link is for a server this device isn't paired with."), findsOneWidget);
      expect(find.text('Servers'), findsOneWidget);
    });

    testWidgets('a link for a paired server offers the switch; a Cancel changes nothing', (tester) async {
      final repo = repoFor();
      final hosts = await pumpHosted(tester, repo, 'aurelius://$_otherHost/ticket/$_ticket', known: true);
      expect(find.text("This link is for Thuis. You're on Segrusa."), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await settleBoard(tester);
      verifyNever(() => hosts.add(any()));
      expect(pendingAureliusLink, isNull);
    });

    testWidgets('switching keeps the link in memory for the other host\'s app', (tester) async {
      addTearDown(() => pendingAureliusLink = null);
      final repo = repoFor();
      final raw = 'aurelius://$_otherHost/ticket/$_ticket';
      final hosts = await pumpHosted(tester, repo, raw, known: true);
      await tester.tap(find.text('Switch to Thuis'));
      await settleBoard(tester);
      expect(pendingAureliusLink, raw);
      expect((verify(() => hosts.add(captureAny())).captured.single as HostSelected).hostId, _otherHost);
    });
  });

  group('a ticket that cannot be reached (§6.4)', () {
    MockRepository failing(ErrorCode code) {
      final repo = boardRepository();
      when(() => repo.host).thenReturn(const Host(hostId: _host, name: 'Segrusa', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
      when(() => repo.ticket(any())).thenThrow(ProtocolException(code, 'no'));
      return repo;
    }

    testWidgets('not found says so with the server\'s name and leads to the Board', (tester) async {
      await pumpBoardPage(tester, failing(ErrorCode.notFound), const TicketPage(ticketId: _ticket));
      expect(find.text('Ticket not found'), findsOneWidget);
      expect(find.text("This ticket doesn't exist on Segrusa. It may have been deleted from the Backlog."), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-go-to-board')), findsOneWidget);
    });

    testWidgets('forbidden says No access', (tester) async {
      await pumpBoardPage(tester, failing(ErrorCode.forbidden), const TicketPage(ticketId: _ticket));
      expect(find.text('No access'), findsOneWidget);
      expect(find.text("You don't have access to this ticket."), findsOneWidget);
    });
  });
}
