import Alpine from './alpine-csp.min.js';
import { priorityFilters } from './home-filters.js';
import { sectionCollapse } from './section-collapse.js';
import { listColorPicker } from './list-color-picker.js';
import { listCreateDialog } from './list-create.js';
import { calendarPreferences, playPreferences } from './general-settings.js';

Alpine.data('listColorPicker', listColorPicker);
Alpine.data('listCreateDialog', listCreateDialog);
Alpine.data('calendarPreferences', calendarPreferences);
Alpine.data('playPreferences', playPreferences);
Alpine.data('priorityFilters', priorityFilters);
Alpine.data('sectionCollapse', sectionCollapse);
Alpine.start();
