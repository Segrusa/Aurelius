import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'host.dart';

/// Paired hosts. Device secrets live in the Keychain (iOS) / Keystore
/// (Android); the host list itself holds no secrets.
abstract class HostStore {
  Future<List<Host>> hosts();
  Future<String?> activeHostId();
  Future<void> setActive(String hostId);
  Future<void> save(Host host, String deviceSecret);
  Future<String?> secret(String hostId);
  Future<void> remove(String hostId);
}

class DeviceHostStore implements HostStore {
  DeviceHostStore({FlutterSecureStorage? secure}) : _secure = secure ?? const FlutterSecureStorage();

  final FlutterSecureStorage _secure;
  static const _hostsKey = 'hosts.v1';
  static const _activeKey = 'hosts.active';

  String _secretKey(String hostId) => 'device_secret.$hostId';

  @override
  Future<List<Host>> hosts() async {
    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString(_hostsKey);
    if (raw == null) return const [];
    final list = jsonDecode(raw) as List<dynamic>;
    return list.map((e) => Host.fromJson(e as Map<String, dynamic>)).toList();
  }

  Future<void> _write(List<Host> hosts) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_hostsKey, jsonEncode(hosts.map((h) => h.toJson()).toList()));
  }

  @override
  Future<String?> activeHostId() async => (await SharedPreferences.getInstance()).getString(_activeKey);

  @override
  Future<void> setActive(String hostId) async => (await SharedPreferences.getInstance()).setString(_activeKey, hostId);

  @override
  Future<void> save(Host host, String deviceSecret) async {
    await _secure.write(
      key: _secretKey(host.hostId),
      value: deviceSecret,
      iOptions: const IOSOptions(accessibility: KeychainAccessibility.first_unlock_this_device),
    );
    final all = [...(await hosts()).where((h) => h.hostId != host.hostId), host];
    await _write(all);
    await setActive(host.hostId);
  }

  @override
  Future<String?> secret(String hostId) => _secure.read(key: _secretKey(hostId));

  @override
  Future<void> remove(String hostId) async {
    await _secure.delete(key: _secretKey(hostId));
    final rest = (await hosts()).where((h) => h.hostId != hostId).toList();
    await _write(rest);
    final prefs = await SharedPreferences.getInstance();
    if (prefs.getString(_activeKey) == hostId) {
      if (rest.isEmpty) {
        await prefs.remove(_activeKey);
      } else {
        await prefs.setString(_activeKey, rest.first.hostId);
      }
    }
  }
}

/// In-memory store for tests.
class MemoryHostStore implements HostStore {
  final List<Host> _hosts = [];
  final Map<String, String> _secrets = {};
  String? _active;

  @override
  Future<List<Host>> hosts() async => List.unmodifiable(_hosts);

  @override
  Future<String?> activeHostId() async => _active;

  @override
  Future<void> setActive(String hostId) async => _active = hostId;

  @override
  Future<void> save(Host host, String deviceSecret) async {
    _hosts.removeWhere((h) => h.hostId == host.hostId);
    _hosts.add(host);
    _secrets[host.hostId] = deviceSecret;
    _active = host.hostId;
  }

  @override
  Future<String?> secret(String hostId) async => _secrets[hostId];

  @override
  Future<void> remove(String hostId) async {
    _hosts.removeWhere((h) => h.hostId == hostId);
    _secrets.remove(hostId);
    if (_active == hostId) _active = _hosts.isEmpty ? null : _hosts.first.hostId;
  }
}
