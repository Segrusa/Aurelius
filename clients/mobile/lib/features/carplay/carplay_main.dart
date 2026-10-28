import 'package:flutter/widgets.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../core/hosts/host_store.dart';
import '../../core/net/connection.dart';
import '../../core/net/gateway_api.dart';
import '../../data/aurelius_repository.dart';
import 'carplay_controller.dart';
import 'carplay_screen.dart';

/// The CarPlay engine's Dart side (`carPlayMain` in main.dart): no widgets,
/// its own connection to the phone's active host, alive while the car is.
Future<void> runCarPlay() async {
  WidgetsFlutterBinding.ensureInitialized();
  final screen = ChannelCarScreen();
  final store = DeviceHostStore();
  final hosts = await store.hosts();
  final activeId = await store.activeHostId();
  final host = hosts.where((h) => h.hostId == activeId).firstOrNull ?? hosts.firstOrNull;
  if (host == null) {
    await screen.lists(const [
      CarSection(items: [CarItem(text: 'Not paired', detail: 'Pair Aurelius on your iPhone first')]),
    ]);
    await screen.speak('Aurelius is not paired with your PC yet. Pair it on your iPhone when you are parked.');
    await screen.voice(VoiceState.offline);
    return;
  }
  final connection = AureliusConnection(host: host, store: store, api: GatewayApi())..start();
  final controller = CarPlayController(repository: AureliusRepository(connection), screen: screen, prefs: await SharedPreferences.getInstance());
  await controller.start();
}
