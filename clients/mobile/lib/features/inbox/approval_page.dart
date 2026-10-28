import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/views.dart';
import '../connection/connection_banner.dart';
import '../connection/connection_bloc.dart';
import 'inbox_bloc.dart';
import 'plan_review.dart';
import 'question_form.dart';
import '../../core/widgets/ios.dart';

/// One concrete action to approve or reject: the same facts as the desktop
///. Approving allows only this exact action, once.
/// ⌘/Ctrl+Enter approves, ⌘/Ctrl+Backspace rejects.
class ApprovalPage extends StatelessWidget {
  const ApprovalPage({super.key, required this.approvalId});

  final String approvalId;

  @override
  Widget build(BuildContext context) {
    return BlocListener<InboxBloc, InboxState>(
      // Leave only once this request was decided (not on any other notice).
      listenWhen: (a, b) => a.deciding.contains(approvalId) && !b.deciding.contains(approvalId),
      listener: (context, state) {
        // A plan's sheet shows what became of it; leaving it is the user's call.
        if (state.planOutcomes.containsKey(approvalId)) return;
        if (context.canPop()) context.pop();
      },
      child: BlocConsumer<InboxBloc, InboxState>(
        listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
        listener: (context, state) {
          toast(context, state.notice!);
          context.read<InboxBloc>().add(const InboxNoticeShown());
        },
        builder: (context, state) {
          final approval = state.approval(approvalId);
          final deciding = state.deciding.contains(approvalId);
          final link = context.watch<ConnectionBloc>().state;
          if (approval == null) {
            return Scaffold(
              appBar: GlassAppBar(),
              body: state.loading
                  ? const Center(child: CircularProgressIndicator())
                  : const EmptyState('This request changed or was already handled', detail: 'Nothing waits for you here anymore.'),
            );
          }
          void decide(bool approve, {bool always = false}) {
            if (deciding || !link.isOnline) return;
            HapticFeedback.mediumImpact();
            context.read<InboxBloc>().add(ApprovalDecided(approval, approve: approve, alwaysAllow: always));
          }

          // Only for MCP tools the server marks eligible (older servers never do).
          final offerAlways = approval.alwaysAllowEligible && link.can('approve');

          final minutes = ((approval.expiresAtMs - DateTime.now().millisecondsSinceEpoch) / 60000).ceil();
          if (approval.isPlan) return PlanLanding(approval: approval);
          if (approval.isQuestion) {
            return QuestionPage(
              approval: approval,
              busy: deciding || !link.isOnline,
              minutes: minutes,
              onSkip: () => decide(false),
              onAnswer: (answers) {
                HapticFeedback.mediumImpact();
                context.read<InboxBloc>().add(QuestionAnswered(approval, answers));
              },
            );
          }
          return CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.enter, meta: true): () => decide(true),
              const SingleActivator(LogicalKeyboardKey.enter, control: true): () => decide(true),
              const SingleActivator(LogicalKeyboardKey.backspace, meta: true): () => decide(false),
              const SingleActivator(LogicalKeyboardKey.backspace, control: true): () => decide(false),
            },
            child: Focus(
              autofocus: true,
              child: Scaffold(
                appBar: GlassAppBar(title: const Text('Approval')),
                body: Column(
                  children: [
                    const ConnectionBanner(),
                    Expanded(
                      child: ListView(
                        padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
                        children: [
                          Text(approval.title, style: TextStyle(color: Tokens.text2)),
                          const SizedBox(height: 4),
                          Text(approval.tool, style: Tokens.title(size: 26)),
                          const SizedBox(height: 16),
                          InfoRow('Effect', approval.effectLabel),
                          if (approval.resources.isNotEmpty) InfoRow('Destination', approval.resources.join('\n')),
                          if (approval.account.isNotEmpty) InfoRow('Account', approval.account),
                          if (approval.reasons.isNotEmpty) InfoRow('Why it asks', approval.reasons.join('\n')),
                          InfoRow('Expires', minutes <= 0 ? 'Expired' : 'In $minutes min. Expiry never counts as permission.'),
                          const Eyebrow('Arguments', padding: EdgeInsets.fromLTRB(0, 16, 0, 8)),
                          Container(
                            width: double.infinity,
                            padding: const EdgeInsets.all(12),
                            decoration: BoxDecoration(
                              color: Tokens.surface,
                              borderRadius: BorderRadius.circular(Tokens.radius),
                              border: Border.all(color: Tokens.border),
                            ),
                            child: SelectableText(approval.argumentsPretty, style: Tokens.monoSmall(color: Tokens.text)),
                          ),
                          const SizedBox(height: 12),
                          Text('Digest ${approval.shortDigest}', style: Tokens.monoSmall()),
                          const SizedBox(height: 6),
                          if (offerAlways)
                            Text(
                              'Approve once allows only this exact action. Always allow lets ${approval.mcpToolLabel} run without asking from now on, '
                              'in every project, and approves its other waiting asks. Plan mode still asks. You can take it back in Settings.',
                              key: const ValueKey('approval-always-note'),
                              style: TextStyle(color: Tokens.muted, fontSize: 12.5),
                            )
                          else
                            Text('Only this exact action, once. No "always allow".', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                        ],
                      ),
                    ),
                    SafeArea(
                      top: false,
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            if (offerAlways)
                              Padding(
                                padding: const EdgeInsets.only(bottom: 8),
                                child: SizedBox(
                                  width: double.infinity,
                                  child: OutlinedButton.icon(
                                    key: const ValueKey('approval-always-allow'),
                                    onPressed: deciding || !link.isOnline || minutes <= 0 ? null : () => decide(true, always: true),
                                    icon: const Icon(Icons.verified_user_outlined, size: 18),
                                    label: const Text('Always allow'),
                                  ),
                                ),
                              ),
                            Row(
                              children: [
                                Expanded(
                                  child: OutlinedButton(onPressed: deciding || !link.isOnline ? null : () => decide(false), child: const Text('Reject')),
                                ),
                                const SizedBox(width: 12),
                                Expanded(
                                  child: FilledButton(
                                    onPressed: deciding || !link.isOnline || minutes <= 0 ? null : () => decide(true),
                                    child: deciding
                                        ? SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2, color: Tokens.bg))
                                        : const Text('Approve once'),
                                  ),
                                ),
                              ],
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}

/// A plan reached through a link or notification: the plan opens in its sheet.
class PlanLanding extends StatefulWidget {
  const PlanLanding({super.key, required this.approval});

  final ApprovalView approval;

  @override
  State<PlanLanding> createState() => _PlanLandingState();
}

class _PlanLandingState extends State<PlanLanding> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) showPlanReview(context, widget.approval);
    });
  }

  @override
  Widget build(BuildContext context) {
    final approval = widget.approval;
    return Scaffold(
      appBar: GlassAppBar(title: const Text('Plan')),
      body: Column(
        children: [
          const ConnectionBanner(),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text(approval.title, style: TextStyle(color: Tokens.text2)),
                  const SizedBox(height: 8),
                  Text(approval.planHeadline, textAlign: TextAlign.center, style: Tokens.title(size: 20)),
                  const SizedBox(height: 20),
                  FilledButton.icon(
                    onPressed: () => showPlanReview(context, approval),
                    icon: const Icon(Icons.article_outlined),
                    label: const Text('Review plan'),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
