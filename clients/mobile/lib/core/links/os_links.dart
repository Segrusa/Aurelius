import 'dart:async';

import 'package:app_links/app_links.dart';

import '../../features/links/link_actions.dart';

// Links the operating system opens in the app (A5b, ui-ontwerp §6.4): Android's
// ACTION_VIEW and iOS's openURLContexts, cold start and warm start, through
// `app_links`. Flutter's own deep linking stays off (manifest, Info.plist): it
// would hand the path to go_router and drop the server id. Here a link is only
// handed on as text; `parseAureliusLink` and the host check decide what it is.

/// Hands every `aurelius:` link from [links] to the app. Other schemes are not
/// ours and are dropped; a malformed one of ours is passed on to say so.
StreamSubscription<Uri> listenForOsLinks(Stream<Uri> links) =>
    links.where((uri) => uri.scheme == 'aurelius').listen((uri) => receiveAureliusLink(uri.toString()));

/// Starts listening to the system. Early in `main`: the first link of a cold start
/// is held by the plugin until something listens.
StreamSubscription<Uri> startOsLinks() => listenForOsLinks(AppLinks().uriLinkStream);
