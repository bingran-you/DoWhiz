import path from 'node:path';
import fs from 'node:fs';
import {
  canonicalToPath,
  loadStructuredRows,
  normalizeKeyword,
  normalizeUrlPath,
  parseArgs,
  parseInteger,
  parseNumber,
  readJsonFile,
  writeTextFile
} from './lib/seo-utils.mjs';

const KEYWORDS_PATH = 'seo/keywords.json';
const CONTENT_REGISTRY_PATH = 'seo/content-registry.json';
const DEFAULT_SEARCH_CONSOLE_PATH = 'seo/metrics/search_console_latest.csv';
const DEFAULT_KEYWORD_VOLUME_PATH = 'seo/metrics/keyword_volume_latest.csv';
const OUTPUT_DIR = 'reports';
const DEFAULT_METRICS_DIR = 'seo/metrics';

const SEARCH_CONSOLE_PATTERNS = [
  /^search[_-]?console.*\.(csv|json)$/i,
  /^gsc.*\.(csv|json)$/i,
  /^google[_-]?search[_-]?console.*\.(csv|json)$/i
];

const KEYWORD_VOLUME_PATTERNS = [
  /^keyword[_-]?volume.*\.(csv|json)$/i,
  /^search[_-]?volume.*\.(csv|json)$/i,
  /^keyword.*search.*\.(csv|json)$/i
];

function findFirstValue(row, keys) {
  for (const key of keys) {
    if (row[key] !== undefined && row[key] !== null && row[key] !== '') {
      return row[key];
    }
  }
  return '';
}

function toRelativePath(absolutePath) {
  return path.relative(process.cwd(), absolutePath).replaceAll(path.sep, '/');
}

function resolveMetricsInput(requestedPath, defaultPath, patterns) {
  if (requestedPath) {
    return {
      requestedPath,
      resolvedPath: requestedPath,
      sourceKind: 'explicit',
      exists: fs.existsSync(path.resolve(process.cwd(), requestedPath))
    };
  }

  const defaultAbsolutePath = path.resolve(process.cwd(), defaultPath);
  if (fs.existsSync(defaultAbsolutePath)) {
    return {
      requestedPath: defaultPath,
      resolvedPath: defaultPath,
      sourceKind: 'default',
      exists: true
    };
  }

  const metricsDirAbsolutePath = path.resolve(process.cwd(), DEFAULT_METRICS_DIR);
  if (!fs.existsSync(metricsDirAbsolutePath)) {
    return {
      requestedPath: defaultPath,
      resolvedPath: defaultPath,
      sourceKind: 'default-missing',
      exists: false
    };
  }

  const bestMatch = fs
    .readdirSync(metricsDirAbsolutePath, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => ({
      entry,
      absolutePath: path.join(metricsDirAbsolutePath, entry.name)
    }))
    .filter(({ entry }) => patterns.some((pattern) => pattern.test(entry.name)))
    .map(({ absolutePath }) => ({
      absolutePath,
      relativePath: toRelativePath(absolutePath),
      modifiedTimeMs: fs.statSync(absolutePath).mtimeMs
    }))
    .sort((left, right) => right.modifiedTimeMs - left.modifiedTimeMs)[0];

  if (bestMatch) {
    return {
      requestedPath: defaultPath,
      resolvedPath: bestMatch.relativePath,
      sourceKind: 'auto-discovered',
      exists: true
    };
  }

  return {
    requestedPath: defaultPath,
    resolvedPath: defaultPath,
    sourceKind: 'default-missing',
    exists: false
  };
}

function aggregateSearchConsoleRows(rawRows) {
  const grouped = new Map();

  for (const row of rawRows) {
    const keyword = normalizeKeyword(findFirstValue(row, ['query', 'keyword']));
    if (!keyword) {
      continue;
    }

    const pagePath = normalizeUrlPath(findFirstValue(row, ['page', 'url', 'target_url']));
    const impressions = parseInteger(findFirstValue(row, ['impressions']), 0);
    const clicks = parseInteger(findFirstValue(row, ['clicks']), 0);
    const ctrValue = parseNumber(findFirstValue(row, ['ctr']), NaN);
    const ctr = Number.isFinite(ctrValue) ? ctrValue : impressions > 0 ? clicks / impressions : 0;
    const position = parseNumber(findFirstValue(row, ['position', 'avg_position']), NaN);

    if (!grouped.has(keyword)) {
      grouped.set(keyword, {
        actual_urls: new Map(),
        clicks: 0,
        impressions: 0,
        weighted_position_total: 0,
        weighted_position_impressions: 0
      });
    }

    const aggregate = grouped.get(keyword);
    aggregate.clicks += clicks;
    aggregate.impressions += impressions;

    if (Number.isFinite(position)) {
      aggregate.weighted_position_total += position * Math.max(impressions, 1);
      aggregate.weighted_position_impressions += Math.max(impressions, 1);
    }

    if (pagePath) {
      const existingPageMetrics = aggregate.actual_urls.get(pagePath) || { clicks: 0, impressions: 0, ctr: 0 };
      const nextImpressions = existingPageMetrics.impressions + impressions;
      const nextClicks = existingPageMetrics.clicks + clicks;
      aggregate.actual_urls.set(pagePath, {
        clicks: nextClicks,
        impressions: nextImpressions,
        ctr: nextImpressions > 0 ? nextClicks / nextImpressions : ctr
      });
    }
  }

  return grouped;
}

function aggregateVolumeRows(rawRows) {
  const grouped = new Map();

  for (const row of rawRows) {
    const keyword = normalizeKeyword(findFirstValue(row, ['keyword', 'query']));
    if (!keyword) {
      continue;
    }

    const volume = parseInteger(findFirstValue(row, ['search_volume', 'avg_monthly_searches', 'volume']), 0);
    const current = grouped.get(keyword);

    if (!current || volume > current.search_volume) {
      grouped.set(keyword, {
        search_volume: volume,
        country: findFirstValue(row, ['country']),
        language: findFirstValue(row, ['language']),
        source: findFirstValue(row, ['source'])
      });
    }
  }

  return grouped;
}

function priorityRank(priority) {
  switch ((priority || '').toLowerCase()) {
    case 'high':
      return 0;
    case 'medium':
      return 1;
    default:
      return 2;
  }
}

function chooseTopPage(actualUrlMetrics) {
  return [...actualUrlMetrics.entries()].sort((left, right) => right[1].impressions - left[1].impressions)[0]?.[0] || '';
}

function buildRecommendation(row) {
  if (row.target_page === '(unassigned)') {
    return 'Create or assign a target page';
  }

  if (row.cannibalization) {
    return 'Consolidate or retarget competing pages';
  }

  if (row.search_console_input_available && row.impressions >= 100 && row.ctr < 0.03) {
    return 'Test title and meta for CTR';
  }

  if (row.search_console_input_available && row.avg_position >= 4 && row.avg_position <= 20) {
    return 'Refresh on-page content and internal links';
  }

  if (row.search_console_input_available && row.avg_position > 20 && row.impressions >= 50) {
    return 'Expand content depth and supporting links';
  }

  if (!row.search_console_input_available && !row.has_volume_data) {
    return 'Collect Search Console and keyword volume data';
  }

  if (!row.search_console_input_available) {
    return 'Collect Search Console data';
  }

  if (!row.has_volume_data) {
    return row.has_search_console_row ? 'Add keyword volume data for prioritization' : 'No ranking data yet; add search volume';
  }

  if (row.clicks === 0 && row.impressions === 0) {
    return 'No ranking data yet';
  }

  return 'Monitor';
}

function formatDecimal(value, digits = 1) {
  if (!Number.isFinite(value)) {
    return '-';
  }
  return value.toFixed(digits);
}

function formatPercent(value) {
  if (!Number.isFinite(value)) {
    return '-';
  }
  return `${(value * 100).toFixed(1)}%`;
}

function buildMarkdownReport({
  generatedAt,
  dataMode,
  searchConsoleInput,
  keywordVolumeInput,
  rows,
  missingTargetPages,
  noVolumeData,
  noSearchConsoleData,
  rankingOpportunities,
  highImpressionLowCtr,
  cannibalizationCandidates
}) {
  const lines = [
    '# SEO Keyword Report',
    '',
    `Generated at: ${generatedAt}`,
    `Search Console input: \`${searchConsoleInput.resolvedPath}\``,
    `Search Console status: ${searchConsoleInput.exists ? `${searchConsoleInput.sourceKind}` : 'missing'}`,
    `Keyword volume input: \`${keywordVolumeInput.resolvedPath}\``,
    `Keyword volume status: ${keywordVolumeInput.exists ? `${keywordVolumeInput.sourceKind}` : 'missing'}`,
    `Data mode: ${dataMode}`,
    '',
    '## Keyword Table',
    '',
    '| Keyword | Target page | Avg position | Impressions | Clicks | CTR | Search volume | Status / action |',
    '| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |'
  ];

  for (const row of rows) {
    lines.push(
      `| ${row.keyword} | ${row.target_page} | ${formatDecimal(row.avg_position)} | ${row.impressions} | ${row.clicks} | ${formatPercent(row.ctr)} | ${row.search_volume_display} | ${row.recommended_action} |`
    );
  }

  const renderKeywordList = (items, formatter) => {
    if (items.length === 0) {
      return ['- (none)'];
    }
    return items.map(formatter);
  };

  lines.push('');
  lines.push('## Missing Target Pages');
  lines.push('');
  lines.push(...renderKeywordList(missingTargetPages, (row) => `- ${row.keyword}: ${row.recommended_action}`));

  lines.push('');
  lines.push('## Keywords With No Search Console Data');
  lines.push('');
  lines.push(...renderKeywordList(noSearchConsoleData, (row) => `- ${row.keyword}: collect Search Console data before ranking analysis.`));

  lines.push('');
  lines.push('## Keywords With No Volume Data');
  lines.push('');
  lines.push(...renderKeywordList(noVolumeData, (row) => `- ${row.keyword}: add search volume before making priority decisions.`));

  lines.push('');
  lines.push('## Keywords Ranking In Positions 4-20');
  lines.push('');
  lines.push(
    ...renderKeywordList(
      rankingOpportunities,
      (row) => `- ${row.keyword}: avg position ${formatDecimal(row.avg_position)}, target ${row.target_page}.`
    )
  );

  lines.push('');
  lines.push('## High-Impression Low-CTR Pages');
  lines.push('');
  lines.push(
    ...renderKeywordList(
      highImpressionLowCtr,
      (row) => `- ${row.keyword}: ${row.impressions} impressions, CTR ${formatPercent(row.ctr)}, target ${row.target_page}.`
    )
  );

  lines.push('');
  lines.push('## Likely Keyword Cannibalization Candidates');
  lines.push('');
  lines.push(
    ...renderKeywordList(
      cannibalizationCandidates,
      (row) =>
        `- ${row.keyword}: tracked target ${row.target_page}, ranking pages ${row.actual_pages.join(', ')}.`
    )
  );

  lines.push('');
  return `${lines.join('\n')}\n`;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const searchConsoleInput = resolveMetricsInput(args['search-console'], DEFAULT_SEARCH_CONSOLE_PATH, SEARCH_CONSOLE_PATTERNS);
  const keywordVolumeInput = resolveMetricsInput(args['keyword-volume'], DEFAULT_KEYWORD_VOLUME_PATH, KEYWORD_VOLUME_PATTERNS);

  const searchConsoleSource = loadStructuredRows(searchConsoleInput.resolvedPath);
  const keywordVolumeSource = loadStructuredRows(keywordVolumeInput.resolvedPath);

  const inventory = readJsonFile(KEYWORDS_PATH).keywords || [];
  const pages = readJsonFile(CONTENT_REGISTRY_PATH).pages || [];
  const pageByPath = new Map(pages.map((page) => [page.url_path, page]));

  const searchGroups = aggregateSearchConsoleRows(searchConsoleSource.rows);
  const volumeGroups = aggregateVolumeRows(keywordVolumeSource.rows);
  const generatedAt = new Date().toISOString();
  const reportDate = generatedAt.slice(0, 10);
  const usesFixtures =
    searchConsoleInput.resolvedPath.includes('/fixtures/') || keywordVolumeInput.resolvedPath.includes('/fixtures/');
  const searchConsoleAvailable = searchConsoleSource.format !== 'missing';
  const keywordVolumeAvailable = keywordVolumeSource.format !== 'missing';
  let dataMode = 'missing-inputs';

  if (usesFixtures && searchConsoleAvailable && keywordVolumeAvailable) {
    dataMode = 'fixture';
  } else if (usesFixtures && searchConsoleAvailable) {
    dataMode = 'fixture-search-console-only';
  } else if (usesFixtures && keywordVolumeAvailable) {
    dataMode = 'fixture-keyword-volume-only';
  } else if (searchConsoleAvailable && keywordVolumeAvailable) {
    dataMode = 'live-or-local';
  } else if (searchConsoleAvailable) {
    dataMode = 'search-console-only';
  } else if (keywordVolumeAvailable) {
    dataMode = 'keyword-volume-only';
  }

  const rows = inventory.map((keywordRow) => {
    const keywordKey = normalizeKeyword(keywordRow.keyword);
    const searchMetrics = searchGroups.get(keywordKey);
    const volumeMetrics = volumeGroups.get(keywordKey);
    const targetPath = canonicalToPath(keywordRow.target_url);
    const targetPage = pageByPath.get(targetPath);
    const actualPages = searchMetrics ? [...searchMetrics.actual_urls.keys()] : [];
    const topRankingPage = searchMetrics ? chooseTopPage(searchMetrics.actual_urls) : '';
    const avgPosition =
      searchMetrics && searchMetrics.weighted_position_impressions > 0
        ? searchMetrics.weighted_position_total / searchMetrics.weighted_position_impressions
        : NaN;
    const impressions = searchMetrics?.impressions || 0;
    const clicks = searchMetrics?.clicks || 0;
    const ctr = impressions > 0 ? clicks / impressions : 0;
    const cannibalization =
      actualPages.length > 1 || (topRankingPage && targetPath && topRankingPage !== targetPath);

    const row = {
      keyword: keywordRow.keyword,
      cluster: keywordRow.cluster,
      priority: keywordRow.priority,
      target_page: targetPath || '(unassigned)',
      target_page_title: targetPage?.title || '',
      search_console_input_available: searchConsoleAvailable,
      keyword_volume_input_available: keywordVolumeAvailable,
      has_search_console_row: Boolean(searchMetrics),
      avg_position: avgPosition,
      impressions,
      clicks,
      ctr,
      search_volume: volumeMetrics?.search_volume ?? null,
      search_volume_display: volumeMetrics?.search_volume ?? '-',
      has_volume_data: Boolean(volumeMetrics && Number.isFinite(volumeMetrics.search_volume) && volumeMetrics.search_volume > 0),
      actual_pages: actualPages,
      top_ranking_page: topRankingPage,
      cannibalization,
      recommended_action: '',
      notes: keywordRow.notes
    };

    row.recommended_action = buildRecommendation(row);
    return row;
  });

  rows.sort((left, right) => {
    const priorityDelta = priorityRank(left.priority) - priorityRank(right.priority);
    if (priorityDelta !== 0) {
      return priorityDelta;
    }
    return right.impressions - left.impressions;
  });

  const missingTargetPages = rows.filter((row) => row.target_page === '(unassigned)');
  const noSearchConsoleData = rows.filter((row) => !row.has_search_console_row);
  const noVolumeData = rows.filter((row) => !row.has_volume_data);
  const rankingOpportunities = rows.filter((row) => Number.isFinite(row.avg_position) && row.avg_position >= 4 && row.avg_position <= 20);
  const highImpressionLowCtr = rows.filter((row) => row.impressions >= 100 && row.ctr < 0.03);
  const cannibalizationCandidates = rows.filter((row) => row.cannibalization);

  const reportPayload = {
    generated_at: generatedAt,
    data_mode: dataMode,
    search_console_input: path.resolve(process.cwd(), searchConsoleInput.resolvedPath),
    search_console_status: {
      requested_path: searchConsoleInput.requestedPath,
      resolved_path: searchConsoleInput.resolvedPath,
      source_kind: searchConsoleInput.sourceKind,
      available: searchConsoleAvailable
    },
    keyword_volume_input: path.resolve(process.cwd(), keywordVolumeInput.resolvedPath),
    keyword_volume_status: {
      requested_path: keywordVolumeInput.requestedPath,
      resolved_path: keywordVolumeInput.resolvedPath,
      source_kind: keywordVolumeInput.sourceKind,
      available: keywordVolumeAvailable
    },
    summary: {
      tracked_keywords: rows.length,
      missing_search_console_data: noSearchConsoleData.length,
      missing_target_pages: missingTargetPages.length,
      missing_volume_data: noVolumeData.length,
      ranking_opportunities: rankingOpportunities.length,
      high_impression_low_ctr: highImpressionLowCtr.length,
      cannibalization_candidates: cannibalizationCandidates.length
    },
    rows,
    sections: {
      no_search_console_data: noSearchConsoleData,
      missing_target_pages: missingTargetPages,
      no_volume_data: noVolumeData,
      ranking_opportunities: rankingOpportunities,
      high_impression_low_ctr: highImpressionLowCtr,
      cannibalization_candidates: cannibalizationCandidates
    }
  };

  const markdown = buildMarkdownReport({
    generatedAt,
    dataMode,
    searchConsoleInput,
    keywordVolumeInput,
    rows,
    missingTargetPages,
    noVolumeData,
    noSearchConsoleData,
    rankingOpportunities,
    highImpressionLowCtr,
    cannibalizationCandidates
  });

  const markdownPath = `${OUTPUT_DIR}/seo-keyword-report-${reportDate}.md`;
  const jsonPath = `${OUTPUT_DIR}/seo-keyword-report-${reportDate}.json`;

  writeTextFile(markdownPath, markdown);
  writeTextFile(jsonPath, `${JSON.stringify(reportPayload, null, 2)}\n`);

  console.log(`Wrote ${markdownPath}`);
  console.log(`Wrote ${jsonPath}`);
}

main();
