import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../core/net/connection.dart';
import '../../data/views.dart';
import 'power_bloc.dart';

int _systemNow() => DateTime.now().millisecondsSinceEpoch;

/// "PC shuts down in 0:45 · Cancel" over the main screens while a shutdown is
/// pending; the countdown ticks each second, the state follows `power.changed`.
class PowerBanner extends StatefulWidget {
  const PowerBanner({super.key, this.now = _systemNow});

  /// Milliseconds since the epoch; replaced in tests.
  final int Function() now;

  @override
  State<PowerBanner> createState() => _PowerBannerState();
}

class _PowerBannerState extends State<PowerBanner> {
  Timer? _tick;

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  void _sync(PendingShutdown? pending) {
    final wants = pending?.deadlineMs != null;
    if (wants && _tick == null) {
      _tick = Timer.periodic(const Duration(seconds: 1), (_) => setState(() {}));
    } else if (!wants && _tick != null) {
      _tick!.cancel();
      _tick = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<PowerBloc>().state;
    final pending = state.pending;
    // Start or stop the ticker after this frame, never while building.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _sync(pending);
    });
    if (pending == null) return const SizedBox.shrink();
    return Padding(
      key: const ValueKey('power-banner'),
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 2),
      child: Glass(
        flat: true,
        radius: 16,
        padding: const EdgeInsets.fromLTRB(14, 4, 4, 4),
        child: Row(
          children: [
            Icon(CupertinoIcons.power, size: 17, color: Tokens.warning),
            const SizedBox(width: 10),
            Expanded(child: Text(pendingText(pending, widget.now()), key: const ValueKey('power-text'), style: TextStyle(fontSize: 14, color: Tokens.label))),
            TextButton(
              key: const ValueKey('power-cancel'),
              onPressed: state.busy ? null : () => context.read<PowerBloc>().add(const PcShutdownCancelled()),
              child: const Text('Cancel'),
            ),
          ],
        ),
      ),
    );
  }
}

/// Settings → "Shut down PC" with Now / When all work is done, each confirmed.
/// Hidden offline and without the Owner right: this is an operating action, so
/// no explanation row. (Every protocol 3.0 server has `power.shutdown`.)
class PowerSection extends StatelessWidget {
  const PowerSection({super.key, required this.link, required this.serverName});

  final LinkState link;
  final String serverName;

  @override
  Widget build(BuildContext context) {
    if (!link.isOnline || PowerBloc.unavailable(link) != null) return const SizedBox.shrink();
    return BlocConsumer<PowerBloc, PowerBlocState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<PowerBloc>().add(const PowerNoticeShown());
      },
      builder: (context, state) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const SizedBox(height: 20),
          InsetGroup(children: [
            _Choice(key: const ValueKey('power-now'), title: 'Shut down PC now', enabled: !state.busy, onTap: () => _ask(context, ShutdownWhen.now)),
            _Choice(key: const ValueKey('power-idle'), title: 'Shut down PC when all work is done', enabled: !state.busy, onTap: () => _ask(context, ShutdownWhen.whenIdle)),
          ]),
        ],
      ),
    );
  }

  Future<void> _ask(BuildContext context, ShutdownWhen when) async {
    final bloc = context.read<PowerBloc>();
    final now = when == ShutdownWhen.now;
    final ok = await glassConfirm(
      context,
      title: now ? 'Shut down $serverName now?' : 'Shut down $serverName when all work is done?',
      body: now
          ? 'The PC that runs $serverName turns off right away. Running answers are lost and this phone cannot reach it until it is switched on again.'
          : 'The PC that runs $serverName turns off once no answers or tasks are running. You can cancel until then.',
      action: 'Shut down',
    );
    if (ok) bloc.add(PcShutdownRequested(when));
  }
}

class _Choice extends StatelessWidget {
  const _Choice({super.key, required this.title, required this.onTap, required this.enabled});

  final String title;
  final VoidCallback onTap;
  final bool enabled;

  @override
  Widget build(BuildContext context) => InkWell(
        onTap: enabled ? onTap : null,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 13),
          child: Text(title, style: TextStyle(fontSize: 17, color: enabled ? Tokens.negative : Tokens.label3)),
        ),
      );
}

/// A confirm on glass that names what it affects; true when confirmed.
Future<bool> glassConfirm(BuildContext context, {required String title, required String body, required String action}) async =>
    await showDialog<bool>(
      context: context,
      builder: (dialog) => Dialog(
        key: const ValueKey('glass-confirm'),
        backgroundColor: Colors.transparent,
        elevation: 0,
        insetPadding: const EdgeInsets.symmetric(horizontal: 28, vertical: 24),
        child: Glass(
          radius: 24,
          padding: const EdgeInsets.fromLTRB(20, 20, 20, 8),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(title, style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600, color: Tokens.label)),
              const SizedBox(height: 8),
              Text(body, style: TextStyle(fontSize: 14.5, height: 1.35, color: Tokens.label2)),
              const SizedBox(height: 8),
              Wrap(
                alignment: WrapAlignment.end,
                children: [
                  TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
                  TextButton(
                    key: const ValueKey('glass-confirm-action'),
                    onPressed: () => Navigator.pop(dialog, true),
                    style: TextButton.styleFrom(foregroundColor: Tokens.negative),
                    child: Text(action),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    ) ??
    false;
