import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_test/flutter_test.dart';

// AUR-FA-M2: what the F1 server says stays word for word; what someone who is
// not the Responsible reads is the same as on the desktop (freeze §5).

TimelineCard _card(String title, String detail, {String actor = 'alex'}) => TimelineCard(actor: actor, atMs: 0, correlationId: 'k1', detail: detail, kind: 'decision', label: 'RESPONSIBLE', seq: 1, title: title);

void main() {
  group('the Activity of a change of Responsible', () {
    for (final sentence in ['alex made bea responsible for #3', 'alex removed ada as responsible for #3', 'alex changed who is responsible for #3']) {
      test('shows "$sentence" as the server wrote it', () {
        expect(activityText(_card(sentence, 'ada was responsible before')), sentence);
      });
    }

    test('a change is not taken for the dispatcher giving up', () {
      expect(dispatcherGaveUp([_card('alex made bea responsible for #3', 'Nobody was responsible before', actor: 'system')]), isFalse);
    });

    test('an older server without the sentence still says who changed it', () {
      expect(activityText(_card('ticket.updated', 'ticket', actor: 'user')), 'A board member changed it');
    });
  });

  group('the line for who is not the Responsible', () {
    final words = {
      TicketState.review: ('Waiting for review', 'Report to review'),
      TicketState.planReview: ('Waiting for plan review', 'Plan to review'),
      TicketState.needsOwner: ('Choose an employee', 'Choose an employee'),
      TicketState.failed: ('✕ Failed', 'Failed'),
      TicketState.waitingForYou: ('Waiting for approval', 'Waiting for you'),
    };
    words.forEach((state, line) {
      test('$state reads "${line.$1}" for others and "${line.$2}" for me', () {
        expect(state.line(others: true), line.$1);
        expect(state.line(), line.$2);
      });
    });
  });
}
