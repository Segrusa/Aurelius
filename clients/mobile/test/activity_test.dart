import 'package:aurelius_mobile/data/activity.dart';
import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:flutter_test/flutter_test.dart';

TicketActivity ticket(String state, {String? waitsFor}) => TicketActivity(ticketId: 't', state: state, employeeName: 'Ann', sinceMs: 1, waitsFor: waitsFor);

void main() {
  test('running, waiting for a slot, paused and waiting for something else read differently', () {
    final lines = [ticket('running'), ticket('waiting_slot', waitsFor: 'slot'), ticket('paused', waitsFor: 'paused'), ticket('waiting_other', waitsFor: 'board_member')].map(ticketActivityLine).toList();
    expect(lines, ['Running: Ann', 'Waiting for a free slot: Ann is busy', 'Paused: Ann', 'Waiting for a board member']);
  });

  test('another board member\'s chat is only a private chat', () {
    expect(workLabel(const ActivityWork(private: true)), privateChatLabel);
    expect(workLabel(const ActivityWork(label: '#2 Work')), '#2 Work');
  });

  test('only slot and machine count as waiting for a slot', () {
    ActivityWaiting w(String why) => ActivityWaiting(employeeName: 'Ann', sinceMs: 1, waitsFor: why, work: const ActivityWork());
    expect(['slot', 'machine', 'paused', 'question'].map(w).where(waitsForSlot).length, 2);
  });
}
