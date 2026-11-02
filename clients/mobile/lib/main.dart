import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'app/app.dart';
import 'core/hosts/host_store.dart';
import 'core/links/os_links.dart';
import 'core/net/gateway_api.dart';
import 'core/push/push_service.dart';
import 'core/widgets/ios.dart';
import 'features/carplay/carplay_main.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  SystemChrome.setSystemUIOverlayStyle(const SystemUiOverlayStyle(systemNavigationBarColor: Colors.transparent, statusBarColor: Colors.transparent));
  startOsLinks();
  runApp(AppearanceScope(child: AureliusApp(store: DeviceHostStore(), api: GatewayApi(), push: PushService())));
}

/// CarPlay's own headless engine starts here (ios/Runner/CarPlaySceneDelegate.swift).
@pragma('vm:entry-point')
void carPlayMain() => runCarPlay();
