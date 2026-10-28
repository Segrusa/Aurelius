import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import 'load_bloc.dart';

/// "Server busy: CPU 82%, waiting" over a conversation whose answer has not
/// started while the server holds new runs. Follows `load_changed`; gone once
/// the answer starts ([waiting] false) or the server lets runs start again.
class LoadBanner extends StatelessWidget {
  const LoadBanner({super.key, required this.waiting});

  /// The conversation waits for an answer that has not started.
  final bool waiting;

  @override
  Widget build(BuildContext context) {
    if (!waiting) return const SizedBox.shrink();
    return BlocProvider(
      create: (context) => LoadBloc(context.read<AureliusRepository>()),
      child: BlocSelector<LoadBloc, LoadState, String?>(
        selector: (s) => s.snapshot?.waitingText,
        builder: (context, text) => text == null
            ? const SizedBox.shrink()
            : Padding(
                key: const ValueKey('load-banner'),
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 2),
                child: Glass(
                  flat: true,
                  radius: 16,
                  padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                  child: Row(
                    children: [
                      Icon(CupertinoIcons.hourglass, size: 17, color: Tokens.warning),
                      const SizedBox(width: 10),
                      Expanded(child: Text(text, style: TextStyle(fontSize: 14, color: Tokens.label))),
                    ],
                  ),
                ),
              ),
      ),
    );
  }
}

/// Settings → PC & security: the server's load (live) and its limits for new
/// runs, read-only. The limits are changed on the desktop.
class LoadLimitsSection extends StatelessWidget {
  const LoadLimitsSection({super.key, required this.link});

  final LinkState link;

  @override
  Widget build(BuildContext context) {
    if (!link.isOnline) return const SizedBox.shrink();
    return BlocProvider(
      create: (context) => LoadBloc(context.read<AureliusRepository>()),
      child: const _LoadBody(),
    );
  }
}

class _LoadBody extends StatelessWidget {
  const _LoadBody();

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<LoadBloc, LoadState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<LoadBloc>().add(const LoadNoticeShown());
      },
      builder: (context, state) {
        final s = state.snapshot;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const GroupHeader('Load limits', caps: true, info: 'The limits for new runs. Change them on the desktop.'),
            if (s == null)
              InsetGroup(children: [
                state.loading
                    ? const Padding(padding: EdgeInsets.all(16), child: Center(child: CupertinoActivityIndicator()))
                    : GroupRow(title: 'Load', subtitle: state.error ?? 'Not available right now.', onTap: () => context.read<LoadBloc>().add(const LoadRefreshed())),
              ])
            else ...[
              _Meter(snapshot: s),
              if (!s.checksActive) const _Footnote('Checks only active on the standard data folder', key: ValueKey('load-checks-inactive')),
              const SizedBox(height: 14),
              _Limits(limits: s.limits),
            ],
          ],
        );
      },
    );
  }
}

/// Live CPU, free memory and runs.
class _Meter extends StatelessWidget {
  const _Meter({required this.snapshot});

  final LoadSnapshot snapshot;

  @override
  Widget build(BuildContext context) {
    final s = snapshot;
    final memUsed = s.memTotalMb <= 0 ? 0.0 : 1 - s.memFreeMb / s.memTotalMb;
    final hot = s.hold?.kind == LoadHoldKind.cpu || s.cpuPct >= s.limits.cpuHigh;
    final low = s.hold?.kind == LoadHoldKind.memory || s.memFreeMb < s.limits.memMinMb;
    return InsetGroup(
      children: [
        _Bar(key: const ValueKey('load-cpu'), title: 'CPU', value: '${s.cpuPct.round()}%', fraction: s.cpuPct / 100, warn: hot),
        _Bar(key: const ValueKey('load-memory'), title: 'Free memory', value: '${gigabytes(s.memFreeMb)} of ${gigabytes(s.memTotalMb)}', fraction: memUsed, warn: low),
        _Bar(key: const ValueKey('load-runs'), title: 'Runs', value: '${s.running} running · ${s.held} waiting'),
        if (s.waitingText != null)
          GroupRow(
            key: const ValueKey('load-hold'),
            leading: Icon(CupertinoIcons.hourglass, size: 20, color: Tokens.warning),
            title: s.waitingText!,
            titleStyle: TextStyle(fontSize: 15, color: Tokens.label),
          ),
      ],
    );
  }
}

class _Bar extends StatelessWidget {
  const _Bar({super.key, required this.title, required this.value, this.fraction, this.warn = false});

  final String title;
  final String value;

  /// Without one, only the value.
  final double? fraction;
  final bool warn;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(children: [
              Text(title, style: TextStyle(fontSize: 17, color: Tokens.label)),
              const SizedBox(width: 12),
              Expanded(
                child: Text(value, textAlign: TextAlign.end, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 17, color: Tokens.label2, fontFeatures: Tokens.tabular)),
              ),
            ]),
            if (fraction != null) ...[
              const SizedBox(height: 8),
              ClipRRect(
                borderRadius: BorderRadius.circular(3),
                child: LinearProgressIndicator(
                  value: fraction!.clamp(0.0, 1.0),
                  minHeight: 6,
                  backgroundColor: Tokens.fill,
                  valueColor: AlwaysStoppedAnimation(warn ? Tokens.warning : Tokens.accent),
                ),
              ),
            ],
          ],
        ),
      );
}

class _Footnote extends StatelessWidget {
  const _Footnote(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
        child: Text(text, style: TextStyle(fontSize: 13, color: Tokens.label2)),
      );
}

/// The limits for new runs, as the server has them.
class _Limits extends StatelessWidget {
  const _Limits({required this.limits});

  final LoadLimits limits;

  @override
  Widget build(BuildContext context) => InsetGroup(
        key: const ValueKey('load-limits'),
        children: [
          GroupRow(title: 'Max runs', subtitle: '0 pauses every new run', value: '${limits.maxRuns}'),
          GroupRow(title: 'CPU high', subtitle: 'Hold new runs from here', value: '${limits.cpuHigh}%'),
          GroupRow(title: 'CPU low', subtitle: 'Start them again below this', value: '${limits.cpuLow}%'),
          GroupRow(title: 'Min free memory', value: '${limits.memMinMb} MB'),
          GroupRow(title: 'CPU and memory checks', subtitle: 'The run limit always counts.', value: limits.enabled ? 'On' : 'Off'),
        ],
      );
}
