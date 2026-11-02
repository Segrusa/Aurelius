import 'dart:async';

import 'package:aurelius_mobile/core/links/os_links.dart';
import 'package:aurelius_mobile/data/links.dart';
import 'package:aurelius_mobile/features/links/link_actions.dart';
import 'package:flutter_test/flutter_test.dart';

// A5b: a link the operating system opens in the app (cold and warm start) is
// handed on as text, held in memory and handled only once the server is connected.

const _host = '01a19c46-232a-75f7-821c-30f835a199c8';
const _ticket = '01a19c47-bfb3-7546-9a78-21e679822c57';
const _link = 'aurelius://$_host/ticket/$_ticket';

void main() {
  late StreamController<Uri> system;
  late StreamSubscription<Uri> listening;
  var taken = 0;

  setUp(() {
    pendingAureliusLink = null;
    taken = 0;
    takePendingAureliusLink = () => taken++;
    system = StreamController<Uri>();
    listening = listenForOsLinks(system.stream);
  });

  tearDown(() async {
    await listening.cancel();
    await system.close();
    pendingAureliusLink = null;
    takePendingAureliusLink = null;
  });

  test('a link from the system is kept as text and the running app is told', () async {
    system.add(Uri.parse(_link));
    await pumpEventQueue();
    expect(pendingAureliusLink, _link);
    expect(parseAureliusLink(pendingAureliusLink!), isA<TicketLink>());
    expect(taken, 1);
  });

  test('a link that arrives before the app is running waits for it', () async {
    takePendingAureliusLink = null;
    system.add(Uri.parse(_link));
    await pumpEventQueue();
    expect(pendingAureliusLink, _link);
  });

  test('the last link wins when two arrive', () async {
    system.add(Uri.parse('aurelius://$_host/message/$_ticket'));
    system.add(Uri.parse(_link));
    await pumpEventQueue();
    expect(pendingAureliusLink, _link);
  });

  test('a link of another scheme is not ours and is dropped', () async {
    system.add(Uri.parse('https://example.com/ticket/$_ticket'));
    await pumpEventQueue();
    expect(pendingAureliusLink, isNull);
    expect(taken, 0);
  });

  test('a malformed aurelius link is passed on, so the app can say it is not valid', () async {
    for (final raw in ['aurelius://$_host/ticket/$_ticket?token=x', 'aurelius://$_host/ticket/$_ticket/', 'aurelius://$_host', 'aurelius:///ticket/$_ticket']) {
      pendingAureliusLink = null;
      system.add(Uri.parse(raw));
      await pumpEventQueue();
      expect(pendingAureliusLink, raw);
      expect(parseAureliusLink(pendingAureliusLink!), isNull, reason: raw);
    }
  });

  test('a valid link waits until the server is connected and signed in; an invalid one does not wait', () {
    expect(aureliusLinkReady(_link, online: false, signedIn: false), isFalse);
    expect(aureliusLinkReady(_link, online: true, signedIn: false), isFalse);
    expect(aureliusLinkReady(_link, online: false, signedIn: true), isFalse);
    expect(aureliusLinkReady(_link, online: true, signedIn: true), isTrue);
    expect(aureliusLinkReady('aurelius://$_host/ticket/$_ticket?token=x', online: false, signedIn: false), isTrue);
  });
}
