/* ================================== *\
|  Gmail Auto-labeller for AO3 emails  |
|  (note: incomplete and unverified)   |
\* ================================== */

// Turn off inbox-only the first few times you run this script, to label all the existing, archived emails.
var inboxOnly = true;
var parentLabel = '📚 AO3';
var newLabel = [parentLabel, '🆕 New Work'].join('/');
var updateLabel = [parentLabel, '♻️ Update'].join('/');
var completeLabel = [parentLabel, '✅ Complete'].join('/');

function getRules()
{
    var isComplete = /Chapters:\s+(\d+)\/\1[^\d]/i;
    var applyComplete = {label: completeLabel, archive: false};
    return [
        {
            name: 'Completed Works',
            filter: {from: 'do-not-reply@archiveofourown.org', subject: {'posted Chapter': true}, match: isComplete},
            apply: applyComplete,
        },
        {
            name: 'Updated Works',
            filter: {from: 'do-not-reply@archiveofourown.org', subject: {'posted Chapter': true}, match: isComplete, negate: true},
            apply: {label: updateLabel, archive: true},
        },
        {
            name: 'Single Chapter Works',
            filter: {from: 'do-not-reply@archiveofourown.org', subject: {'posted': true, 'Chapter': false}, match: isComplete},
            apply: applyComplete,
        },
        {
            name: 'New Incomplete Works',
            // Remove the RegEx match if you want single chapter fics to also have the new label.
            filter: {from: 'do-not-reply@archiveofourown.org', subject: {'posted': true, 'Chapter': false}, match: isComplete, negate: true},
            apply: {label: newLabel, archive: true},
        },
    ];
}

function autoLabelEmail()
{
    var rules = getRules();
    rules.forEach(function (rule) {
        if (typeof rule === 'object') {
            if (typeof rule.name === 'string') {
                Logger.log("Applying rule: %s", rule.name);
            }
            applyRuleToInbox(rule);
        } else {
          Logger.log('Invalid rule supplied.');
        }
    });
}

function applyRuleToInbox(rule)
{
    if (typeof rule.filter !== 'object' || rule.filter.length === 0 || typeof rule.apply !== 'object' || rule.apply.length === 0) {
        return;
    }

    var query = buildSearchQuery(rule.filter);
    var threads = GmailApp.search(query);
    Logger.log(`Found ${threads.length} threads with search:\n${query}`);
    threads.forEach(function (thread) {
        if (doesThreadMatchRule(thread, rule.filter)) {
            applyToThread(thread, rule.apply);
        }
    });
}

/** @param {from?: string, subject?: object<string, bool>} filter */
function buildSearchQuery(filter)
{
    var build = (key, value, include) => (typeof include === 'boolean' && !include ? '-' : '') + key + ':' + '{"' + value + '"}';
    var parts = inboxOnly ? ['in:inbox'] : [];

    // Don't process emails that have already been labelled, it's a waste and will just slow the script down.
    parts.push(build('label', newLabel, false));
    parts.push(build('label', updateLabel, false));
    parts.push(build('label', completeLabel, false));

    if (typeof filter.from === 'string') {
        parts.push(build('from', filter.from, true));
    }

    if (typeof filter.subject === 'object') {
        for (var subject of Object.keys(filter.subject)) {
            if (typeof subject === 'string') {
                parts.push(build('subject', subject, filter.subject[subject]));
            }
        }
    }

    return parts.length > 0 ? parts.join(' AND ') : '';
}

/** @param {match?: RegExp, negate?: bool} filter */
function doesThreadMatchRule(thread, filter)
{
    if (typeof filter.match !== 'object' || typeof filter.match.test !== 'function') {
        return true;
    }
    var isMatch = false;
    var messages = thread.getMessages();
    for (var i = 0; i < messages.length; i++) {
        if (filter.match.test(messages[i].getPlainBody())) {
            isMatch = true;
            break;
        }
    }
    if (filter.negate) {
        return !isMatch;
    }
    return isMatch;
}

/** @param {label?: string, archive?: bool} actions */
function applyToThread(thread, actions)
{
    if (typeof actions.label === 'string') {
        var gmailLabel = GmailApp.getUserLabelByName(actions.label);
        if (gmailLabel) {
            gmailLabel.addToThread(thread);
            Logger.log("Applied label '%s' to thread with subject: %s", actions.label, thread.getFirstMessageSubject());
        } else {
            // TODO: Create the label if it doesn't exist.
        }
    }
    if (typeof actions.archive === 'boolean' && actions.archive) {
        GmailApp.moveThreadToArchive(thread);
        Logger.log("Archived thread with subject: %s", thread.getFirstMessageSubject());
    }
}
