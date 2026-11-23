import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:intl/intl.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart' show confirm;
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart' show toast;
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';

/// A paired device belongs to the board member who paired it (protocol 5, additive).
const ownDevicesFeature = 'own_devices';

/// "Android · Connected now · Notifications on": where a paired device stands, with the
/// desktop's words.
String deviceLine(RemoteClientView d, {DateTime? now}) {
  final seen = d.lastSeenMs;
  final state = d.connected
      ? 'Connected now'
      : seen == null
          ? 'Never connected'
          : 'Last seen ${DateFormat('MMM d').format(DateTime.fromMillisecondsSinceEpoch(seen))}';
  final platform = switch (d.platform) {
    'ios' => 'iPhone',
    'android' => 'Android',
    'macos' => 'Mac',
    'windows' => 'Windows',
    'linux' => 'Linux',
    'web' => 'Browser',
    final other => other,
  };
  return [platform, state, if (d.push) 'Notifications on'].join(' · ');
}

/// Profile → Account → Your devices: the phones and computers this board member
/// paired, newest first; nobody else sees them. This phone is marked and is not
/// revoked from here (Sign out instead). Pairing a new device is done on the PC.
class DevicesPage extends StatefulWidget {
  const DevicesPage({super.key});

  @override
  State<DevicesPage> createState() => _DevicesPageState();
}

class _DevicesPageState extends State<DevicesPage> {
  List<RemoteClientView>? _devices;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final devices = await context.read<AureliusRepository>().devices();
      if (mounted) {
        setState(() {
          _devices = devices;
          _error = null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = e is ProtocolException ? e.shown : '$e');
    }
  }

  Future<bool> _revoke(RemoteClientView d) async {
    final repository = context.read<AureliusRepository>();
    final ok = await confirm(context, title: 'Revoke ${d.name}?', body: 'Everyone on it is signed out, and it must be paired again.', action: 'Revoke', danger: true);
    if (!ok || !mounted) return false;
    try {
      await repository.revokeDevice(d.id);
      if (mounted) setState(() => _devices = [...?_devices?.where((x) => x.id != d.id)]);
      return true;
    } catch (e) {
      if (mounted) toast(context, e is ProtocolException ? e.shown : '$e');
      return false;
    }
  }

  @override
  Widget build(BuildContext context) {
    final devices = _devices;
    final self = context.read<AureliusRepository>().host.clientId;
    return LargeTitleScaffold(
      title: 'Your devices',
      onRefresh: _load,
      fill: devices == null
          ? Center(
              child: _error == null
                  ? const CircularProgressIndicator()
                  : Padding(
                      padding: const EdgeInsets.all(24),
                      child: Column(mainAxisSize: MainAxisSize.min, children: [
                        Text(_error!, key: const ValueKey('devices-error'), textAlign: TextAlign.center, style: TextStyle(color: Tokens.label2)),
                        TextButton(onPressed: _load, child: const Text('Try again')),
                      ]),
                    ),
            )
          : null,
      children: devices == null
          ? null
          : [
              const GroupHeader('Only you see these devices'),
              InsetGroup(
                dividerIndent: 56,
                children: [
                  if (devices.isEmpty) const GroupRow(title: 'No paired devices.'),
                  for (final d in devices)
                    d.id == self
                        ? GroupRow(
                            key: ValueKey('device-${d.id}'),
                            leading: const Icon(CupertinoIcons.device_phone_portrait),
                            title: d.name,
                            subtitle: deviceLine(d),
                            value: 'This phone',
                          )
                        : Dismissible(
                            key: ValueKey('device-${d.id}'),
                            direction: DismissDirection.endToStart,
                            confirmDismiss: (_) => _revoke(d),
                            background: Container(
                              color: Tokens.negative,
                              alignment: Alignment.centerRight,
                              padding: const EdgeInsets.only(right: 20),
                              child: const Text('Revoke', style: TextStyle(color: Colors.white, fontWeight: FontWeight.w600)),
                            ),
                            child: GroupRow(
                              leading: Icon(d.platform == 'ios' || d.platform == 'android' ? CupertinoIcons.device_phone_portrait : CupertinoIcons.desktopcomputer),
                              title: d.name,
                              subtitle: deviceLine(d),
                              trailing: TextButton(key: ValueKey('revoke-device-${d.id}'), onPressed: () => _revoke(d), child: const Text('Revoke')),
                            ),
                          ),
                ],
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
                child: Text('Add a device in Aurelius on the PC: Settings › Account › Your devices.', style: TextStyle(color: Tokens.label2, fontSize: 13)),
              ),
            ],
    );
  }
}
