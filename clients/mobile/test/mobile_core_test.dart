import 'package:aurelius_mobile/app/mobile_core.dart';
import 'package:aurelius_mobile/app/router.dart';
import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

const _host = Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0);

List<String> _paths(List<RouteBase> routes, [String parent = '']) => [
      for (final r in routes) ...switch (r) {
        GoRoute(:final path) => [
            path.startsWith('/') ? path : '$parent/$path',
            ..._paths(r.routes, path.startsWith('/') ? path : '$parent/$path'),
          ],
        StatefulShellRoute(:final branches) => [for (final b in branches) ..._paths(b.routes, parent)],
        _ => _paths(r.routes, parent),
      },
    ];

void main() {
  late AppRouter router;

  setUp(() => router = AppRouter(_host));
  tearDown(() => router.dispose());

  test('the menus leave out the screens of the mobile core list and keep the core', () {
    for (final id in ['plugins', 'artifacts', 'vault', 'usage', 'context', 'company', 'load', 'power', 'runtime', 'advanced', 'recurring', 'tasks', 'projects', 'ideas', 'updates']) {
      expect(shownInMobileCore(id), isFalse, reason: id);
    }
    for (final id in ['team', 'inbox', 'board', 'settings', 'hosts', 'account']) {
      expect(shownInMobileCore(id), isTrue, reason: id);
    }
  });

  test('a screen that is hidden from the menus keeps its route, so a link or a push still opens it', () {
    final paths = _paths(router.config.configuration.routes);
    expect(paths, containsAll(['/usage', '/plugins', '/vault', '/company', '/tasks', '/projects', '/ideas', '/updates', '/context/:runId', '/settings/advanced', '/settings/recurring']));
    for (final link in ['/usage', '/vault?project=p', '/company?view=chart', '/context/r-1', '/settings/recurring', '/project/p/task/t']) {
      expect(router.config.configuration.findMatch(Uri.parse(link)).isError, isFalse, reason: link);
    }
  });

  test('a test of a hidden screen can bring it back with an empty set, and the constant is restored', () {
    mobileCoreHidden = const {};
    expect(shownInMobileCore('vault'), isTrue);
    mobileCoreHidden = kHiddenInMobileCore;
    expect(shownInMobileCore('vault'), isFalse);
  });

  test('the unused features of ticket #19 are in the one list, and what stays is not', () {
    for (final id in ['notes', 'artifacts', 'shares', 'variants', 'dependencies', 'heartbeats', 'api_keys', 'app_keys', 'plugins', 'chat_attach']) {
      expect(shownInMobileCore(id), isFalse, reason: id);
    }
    for (final id in ['webhooks', 'functions', 'ticket_attachments']) {
      expect(kHiddenInMobileCore.contains(id), isFalse, reason: id);
    }
  });
}
