import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

/**
 * Creating a sidebar enables you to:
 - create an ordered group of docs
 - render a sidebar for each doc of that group
 - provide next/previous navigation

 The sidebars can be generated from the filesystem, or explicitly defined here.

 Create as many sidebars as you want.
 */
const sidebars: SidebarsConfig = {
  tutorialSidebar: [
    // Getting Started
    'intro',
    'installation',

    // Configuration Section
    {
      type: 'category',
      label: 'Configuration',
      items: [
        'configuration/index',
        'update-policies',
        'configuration/deployments',
        'configuration/statefulsets',
        'configuration/daemonsets',
        'configuration/helmreleases',
        'configuration/event-sources',
        'configuration/approval-workflow',
        'configuration/notifications',
        'configuration/rollback',
        'configuration/observability',
        'configuration/web-ui',
      ],
    },

    // Guides Section
    {
      type: 'category',
      label: 'Guides',
      items: [
        'guides/helm-installation',
        'guides/update-requests',
        'guides/kubectl-plugin',
        'guides/web-ui',
        'guides/web-ui-authentication',
        'guides/observability-dashboard',
      ],
    },

    // API Section
    {
      type: 'category',
      label: 'API',
      items: [
        'api/index',
        'api/metrics',
      ],
    },
  ],
};

export default sidebars;
