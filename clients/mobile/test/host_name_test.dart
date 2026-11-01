import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/hosts/hosts_bloc.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_daemon.dart';

const _owner = ['read', 'chat', 'task_control', 'approve', 'owner'];
const _me = UserView(createdAtMs: 0, id: 'u-1', username: 'sebastien');

LinkState linkOf({List<String> features = const [serverNameFeature], List<String> capabilities = _owner, UserView? user = _me}) => LinkState(status: LinkStatus.online, features: features, capabilities: capabilities).withBoardMember(exists: true, user: user);

void main() {
  group('the connection keeps Host.name (SN-02)', () {
    late FakeDaemon daemon;
    late MemoryHostStore store;
    late AureliusConnection connection;

    Future<void> connect({bool signedIn = true}) async {
      store = await storeWithSecret();
      daemon.addUser('u-1', 'sebastien', 'a long password');
      if (signedIn) await store.setSessionToken(testHost.hostId, daemon.sessionFor('u-1'));
      connection = AureliusConnection(host: testHost, store: store, api: FakeApi(), connect: daemon.connect)..start();
      await until(() => connection.state.isOnline);
    }

    setUp(() => daemon = FakeDaemon()..features = const [serverNameFeature]);
    tearDown(() => connection.dispose());

    test("a hello with another name overwrites the stored one, matched on host_id (AC2)", () async {
      daemon.chosenName = 'Office';
      await connect(signedIn: false);
      expect(connection.host.name, 'Office');
      expect(connection.state.hostName, 'Office');
      expect((await store.hosts()).single.name, 'Office');
      expect((await store.hosts()).single.url, testHost.url, reason: 'only the name changes (A-02)');
    });

    test('a hello from an older server without the feature still brings its name (AC2)', () async {
      daemon
        ..features = const []
        ..computerName = 'new-pc';
      await connect(signedIn: false);
      expect((await store.hosts()).single.name, 'new-pc');
      expect(daemon.nameReads, 0, reason: 'no server.get_name without the feature (AC4)');
    });

    test('the rename of another host in the store leaves this one alone', () async {
      await connect(signedIn: false);
      await store.rename('another-host', 'Office');
      expect((await store.hosts()).single.name, 'my-pc');
    });

    test('server.renamed reads the name again and stores it, without reconnecting (AC1)', () async {
      await connect();
      final names = <ServerNameView>[];
      final sub = connection.serverNames.listen(names.add);
      daemon
        ..chosenName = 'Thuis-PC'
        ..nameVersion = 1;
      daemon.event(daemon.current!, 'server.renamed', seq: 31);
      await until(() => connection.host.name == 'Thuis-PC');
      expect(daemon.nameReads, 1);
      expect(connection.state.hostName, 'Thuis-PC');
      expect((await store.hosts()).single.name, 'Thuis-PC');
      await until(() => names.isNotEmpty);
      expect((names.single.name, names.single.chosen, names.single.computerName), ('Thuis-PC', 'Thuis-PC', 'my-pc'));
      expect(daemon.sockets, hasLength(1), reason: 'the same connection');
      await sub.cancel();
    });

    test('without the feature, server.renamed is not followed by server.get_name (AC4)', () async {
      daemon.features = const [];
      await connect();
      daemon.event(daemon.current!, 'server.renamed', seq: 31);
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(daemon.nameReads, 0);
    });

    test('set_name: the receipt renames the host at once; empty brings the computer back', () async {
      daemon.capabilities = _owner;
      await connect();
      final repository = AureliusRepository(connection);
      expect((await repository.setServerName('  Kantoor  ')).name, 'Kantoor');
      expect(connection.host.name, 'Kantoor');
      expect((await store.hosts()).single.name, 'Kantoor');
      expect((await repository.setServerName('   ')).name, 'my-pc');
      expect(connection.host.name, 'my-pc');
      expect((await store.hosts()).single.name, 'my-pc');
    });

    test('set_name: a refusal keeps the name and carries the server text', () async {
      daemon.capabilities = _owner;
      await connect();
      final repository = AureliusRepository(connection);
      await expectLater(repository.setServerName('x' * 41), throwsA(isA<ProtocolException>().having((e) => e.shown, 'shown', 'The server name has at most 40 characters.')));
      expect(connection.host.name, 'my-pc');
    });
  });

  group('the paired PCs follow a rename', () {
    test('HostsChanged reloads the list from the store', () async {
      final store = await storeWithSecret();
      final bloc = HostsBloc(store)..add(const HostsStarted());
      await until(() => !bloc.state.loading);
      await store.rename(testHost.hostId, 'Office');
      bloc.add(const HostsChanged());
      await until(() => bloc.state.active?.name == 'Office');
      expect(bloc.state.hosts.single.name, 'Office');
      await bloc.close();
    });
  });

  group('who may rename (SN-01 AC5)', () {
    test('only with the feature, a board member and the Owner right', () {
      expect(linkOf().canRenameServer, isTrue);
      expect(linkOf(features: const []).canRenameServer, isFalse);
      expect(linkOf(user: null).canRenameServer, isFalse);
      expect(linkOf(capabilities: const ['read', 'chat', 'task_control', 'approve']).canRenameServer, isFalse);
    });
  });
}
