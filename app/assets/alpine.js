import Alpine from './alpine-csp.min.js';
import { listPreferences } from './list-preferences.js';
import { dismissibleMenu } from './menu.js';
import { searchOmnibar } from './search-dialog.js';
import { confirmationDialog } from './confirmation-dialog.js';
import { exportDialog } from './export-dialog.js';
import { workspaceCreateDialog, workspaceActionDialog } from './workspace-dialogs.js';
import { capturePreferences } from './capture-preferences.js';
import { mutedLists } from './notification-picker.js';
import { notificationPreferences } from './settings-feedback.js';
import { priorityFilters } from './home-filters.js';
import { sectionCollapse } from './section-collapse.js';
import { listColorPicker } from './list-color-picker.js';
import { listCreateDialog } from './list-create.js';
import { calendarPreferences, playPreferences } from './general-settings.js';

Alpine.data('dismissibleMenu', dismissibleMenu);
Alpine.data('searchOmnibar', searchOmnibar);
Alpine.data('listPreferences', listPreferences);
Alpine.data('listColorPicker', listColorPicker);
Alpine.data('listCreateDialog', listCreateDialog);
Alpine.data('calendarPreferences', calendarPreferences);
Alpine.data('playPreferences', playPreferences);
Alpine.data('priorityFilters', priorityFilters);
Alpine.data('sectionCollapse', sectionCollapse);
Alpine.data('mutedLists', mutedLists);
Alpine.data('notificationPreferences', notificationPreferences);
Alpine.data('capturePreferences', capturePreferences);
Alpine.data('confirmationDialog', confirmationDialog);
Alpine.data('exportDialog', exportDialog);
Alpine.data('workspaceCreateDialog', workspaceCreateDialog);
Alpine.data('workspaceActionDialog', workspaceActionDialog);
// Mark the initial swap containers too, so Alpine cleans up their descendants
// when HTMX removes a parent that has no x-data of its own.
Alpine.addRootSelector(() => '#main, #navigation, [data-app-dialogs]');
Alpine.start();
