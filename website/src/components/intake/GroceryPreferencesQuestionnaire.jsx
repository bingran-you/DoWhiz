import { useState, useCallback, useMemo } from 'react';
import './GroceryPreferencesQuestionnaire.css';
import { useTranslation } from './groceryTranslations';

const STEP_IDS = ['profile', 'shopping', 'categories', 'category_prefs', 'taste', 'budget'];

const CULTURAL_BACKGROUNDS = [
  { value: 'mainland_china', label: '中国大陆' },
  { value: 'taiwan_hk_macau', label: '台湾/港澳' },
  { value: 'abc', label: 'ABC/华裔美国人' },
  { value: 'korea_japan', label: '韩国/日本' },
  { value: 'southeast_asia', label: '东南亚' },
  { value: 'other_asian', label: '其他亚裔' },
  { value: 'non_asian', label: '非亚裔但喜欢亚洲食品' }
];

const HOUSEHOLD_SIZES = [
  { value: '1', label: '1人（自己）' },
  { value: '2', label: '2人（情侣/室友）' },
  { value: '3-4', label: '3-4人（小家庭）' },
  { value: '5+', label: '5人以上（大家庭）' }
];

const TRANSPORT_OPTIONS = [
  { value: 'no_car', label: '没有车，靠公共交通/走路' },
  { value: 'car_15min', label: '有车，15分钟内' },
  { value: 'car_30min', label: '有车，30分钟内' },
  { value: 'car_60min', label: '有车，1小时内也可以' }
];

const SHOPPING_PREFERENCES = [
  { value: 'delivery', label: '网购送货（Weee/Instacart）' },
  { value: 'in_store', label: '线下超市' },
  { value: 'both', label: '两者都可以' }
];

const MEMBERSHIPS = [
  { value: 'sams_club', label: "Sam's Club" },
  { value: 'costco', label: 'Costco' },
  { value: 'kroger_plus', label: 'Kroger Plus Card' }
];

const PREFERRED_STORES = [
  // Asian grocery - online
  { value: 'weee', label: 'Weee (网购)', category: 'asian_online' },
  { value: 'yami', label: 'Yami 亚米 (网购)', category: 'asian_online' },
  // Asian grocery - physical
  { value: '168_asian_mart', label: '168 Asian Mart', category: 'asian_physical' },
  { value: 'hmart', label: 'H Mart 韩亚龙', category: 'asian_physical' },
  { value: 'great_wall', label: 'Great Wall 大中华', category: 'asian_physical' },
  { value: '99_ranch', label: '99 Ranch 大华', category: 'asian_physical' },
  { value: 'mitsuwa', label: 'Mitsuwa 日本超市', category: 'asian_physical' },
  // Warehouse clubs
  { value: 'sams_club', label: "Sam's Club 山姆", category: 'warehouse' },
  { value: 'costco', label: 'Costco 开市客', category: 'warehouse' },
  // Mainstream US
  { value: 'kroger', label: 'Kroger', category: 'mainstream' },
  { value: 'walmart', label: 'Walmart', category: 'mainstream' },
  { value: 'target', label: 'Target', category: 'mainstream' },
  { value: 'aldi', label: 'Aldi', category: 'mainstream' },
  { value: 'trader_joes', label: "Trader Joe's", category: 'mainstream' },
  { value: 'whole_foods', label: 'Whole Foods', category: 'mainstream' }
];

// Pre-computed store lists by category (avoids filtering on every render)
const STORES_ASIAN_ONLINE = PREFERRED_STORES.filter(s => s.category === 'asian_online');
const STORES_ASIAN_PHYSICAL = PREFERRED_STORES.filter(s => s.category === 'asian_physical');
const STORES_WAREHOUSE = PREFERRED_STORES.filter(s => s.category === 'warehouse');
const STORES_MAINSTREAM = PREFERRED_STORES.filter(s => s.category === 'mainstream');

const MAIN_CATEGORIES = [
  { value: 'meat', label: '肉类（猪/牛/羊/鸡）', labelEn: 'Meat' },
  { value: 'seafood', label: '海鲜', labelEn: 'Seafood' },
  { value: 'vegetables', label: '蔬菜', labelEn: 'Vegetables' },
  { value: 'fruits', label: '水果', labelEn: 'Fruits' },
  { value: 'snacks', label: '零食饮料', labelEn: 'Snacks & Drinks' },
  { value: 'hotpot', label: '火锅/烧烤食材', labelEn: 'Hotpot/BBQ' },
  { value: 'condiments', label: '调味料/酱料', labelEn: 'Condiments' },
  { value: 'instant', label: '速食/方便面', labelEn: 'Instant Food' },
  { value: 'dairy', label: '奶制品/鸡蛋', labelEn: 'Dairy/Eggs' },
  { value: 'bakery', label: '面包/烘焙', labelEn: 'Bakery' }
];

const DIETARY_RESTRICTIONS = [
  { value: 'none', label: '无' },
  { value: 'vegetarian', label: '素食/纯素' },
  { value: 'no_pork', label: '不吃猪肉（宗教原因）' },
  { value: 'lactose', label: '乳糖不耐受' },
  { value: 'gluten', label: '麸质过敏' }
];

const BUDGET_MINDSETS = [
  { value: 'price_first', label: '能省则省，价格最重要' },
  { value: 'value', label: '性价比优先，质量也要看' },
  { value: 'quality_first', label: '质量优先，价格其次' },
  { value: 'no_concern', label: '不太在意价格' }
];

const PRIORITY_FACTORS = [
  { value: 'price', label: '价格便宜' },
  { value: 'quality', label: '产品新鲜/质量好' },
  { value: 'convenience', label: '距离近/方便' },
  { value: 'variety', label: '品种齐全' }
];

function ProgressBar({ currentStep, totalSteps }) {
  const progress = ((currentStep + 1) / totalSteps) * 100;
  return (
    <div className="gq-progress-container">
      <div className="gq-progress-bar" style={{ width: `${progress}%` }} />
      <span className="gq-progress-text">
        {currentStep + 1} / {totalSteps}
      </span>
    </div>
  );
}

function SingleSelect({ options, value, onChange, name }) {
  return (
    <div className="gq-option-group">
      {options.map((opt) => (
        <label
          key={opt.value}
          className={`gq-option-btn ${value === opt.value ? 'selected' : ''}`}
        >
          <input
            type="radio"
            name={name}
            value={opt.value}
            checked={value === opt.value}
            onChange={(e) => onChange(e.target.value)}
          />
          <span>{opt.label}</span>
        </label>
      ))}
    </div>
  );
}

function MultiSelect({ options, values, onChange, name }) {
  const handleToggle = (optValue) => {
    if (values.includes(optValue)) {
      onChange(values.filter((v) => v !== optValue));
    } else {
      onChange([...values, optValue]);
    }
  };

  return (
    <div className="gq-option-group gq-multi">
      {options.map((opt) => (
        <label
          key={opt.value}
          className={`gq-option-btn ${values.includes(opt.value) ? 'selected' : ''}`}
        >
          <input
            type="checkbox"
            name={name}
            value={opt.value}
            checked={values.includes(opt.value)}
            onChange={() => handleToggle(opt.value)}
          />
          <span>{opt.label}</span>
        </label>
      ))}
    </div>
  );
}

function RangeSlider({ value, onChange, min = 1, max = 5, labels }) {
  return (
    <div className="gq-slider-container">
      <input
        type="range"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="gq-slider"
      />
      <div className="gq-slider-labels">
        {labels.map((label, idx) => (
          <span key={idx} className={idx + 1 === value ? 'active' : ''}>
            {label}
          </span>
        ))}
      </div>
    </div>
  );
}

function DraggablePriorityList({ items, order, onChange, hint }) {
  const [draggedIdx, setDraggedIdx] = useState(null);

  const handleDragStart = useCallback((idx) => {
    setDraggedIdx(idx);
  }, []);

  const handleDragEnd = useCallback(() => {
    setDraggedIdx(null);
  }, []);

  // handleDragOver needs draggedIdx in deps since it reads current drag state
  const handleDragOver = useCallback((e, idx) => {
    e.preventDefault();
    if (draggedIdx === null || draggedIdx === idx) return;

    const newOrder = [...order];
    const [removed] = newOrder.splice(draggedIdx, 1);
    newOrder.splice(idx, 0, removed);
    onChange(newOrder);
    setDraggedIdx(idx);
  }, [draggedIdx, order, onChange]);

  const itemMap = useMemo(() => {
    const map = {};
    items.forEach((item) => {
      map[item.value] = item;
    });
    return map;
  }, [items]);

  return (
    <div className="gq-priority-list">
      <p className="gq-hint">{hint}</p>
      {order.map((value, idx) => (
        <div
          key={value}
          className={`gq-priority-item ${draggedIdx === idx ? 'dragging' : ''}`}
          draggable
          onDragStart={() => handleDragStart(idx)}
          onDragOver={(e) => handleDragOver(e, idx)}
          onDragEnd={handleDragEnd}
        >
          <span className="gq-priority-rank">{idx + 1}</span>
          <span className="gq-priority-label">{itemMap[value]?.label}</span>
          <span className="gq-drag-handle">⋮⋮</span>
        </div>
      ))}
    </div>
  );
}

function GroceryPreferencesQuestionnaire({
  onComplete,
  onCancel,
  initialData = {},
  locale = 'zh-CN'
}) {
  const { t } = useTranslation(locale);
  const [currentStep, setCurrentStep] = useState(0);
  const [formData, setFormData] = useState({
    // Step 1: Profile
    cultural_background: initialData.cultural_background || '',
    zip_code: initialData.zip_code || '',
    city: initialData.city || '',
    household_size: initialData.household_size || '',

    // Step 2: Shopping Habits
    transport: initialData.transport || '',
    shopping_preference: initialData.shopping_preference || '',
    memberships: initialData.memberships || [],
    preferred_stores: initialData.preferred_stores || [],
    other_stores: initialData.other_stores || '',
    other_membership: initialData.other_membership || '',

    // Step 3: Categories
    main_categories: initialData.main_categories || [],

    // Step 4: Category Preferences (dynamic based on step 3)
    meat_type: initialData.meat_type || [],
    meat_processing: initialData.meat_processing || '',
    meat_quantity: initialData.meat_quantity || '',
    snack_flavor: initialData.snack_flavor || [],
    snack_brands_like: initialData.snack_brands_like || '',
    snack_brands_avoid: initialData.snack_brands_avoid || '',
    vegetable_types: initialData.vegetable_types || '',
    vegetable_organic: initialData.vegetable_organic || '',
    hotpot_frequency: initialData.hotpot_frequency || '',
    hotpot_base: initialData.hotpot_base || [],
    hotpot_brands: initialData.hotpot_brands || '',

    // Step 5: Taste Profile
    sweetness: initialData.sweetness || 3,
    spiciness: initialData.spiciness || 3,
    saltiness: initialData.saltiness || 'normal',
    american_sweets_opinion: initialData.american_sweets_opinion || '',
    avoid_foods: initialData.avoid_foods || '',
    dietary_restrictions: initialData.dietary_restrictions || [],
    other_dietary: initialData.other_dietary || '',

    // Step 6: Budget
    budget_mindset: initialData.budget_mindset || '',
    priority_order: initialData.priority_order || ['quality', 'price', 'convenience', 'variety']
  });

  const updateField = useCallback((field, value) => {
    setFormData((prev) => ({ ...prev, [field]: value }));
  }, []);

  const canProceed = useMemo(() => {
    switch (currentStep) {
      case 0: // Profile
        return formData.cultural_background && formData.zip_code && formData.household_size;
      case 1: // Shopping
        return formData.transport && formData.shopping_preference && formData.preferred_stores.length > 0;
      case 2: // Categories
        return formData.main_categories.length > 0;
      case 3: // Category prefs - optional
        return true;
      case 4: // Taste
        return formData.sweetness && formData.spiciness;
      case 5: // Budget
        return formData.budget_mindset;
      default:
        return true;
    }
  }, [currentStep, formData]);

  const handleNext = () => {
    if (currentStep < STEP_IDS.length - 1) {
      setCurrentStep(currentStep + 1);
    } else {
      onComplete?.(formData);
    }
  };

  const handleBack = () => {
    if (currentStep > 0) {
      setCurrentStep(currentStep - 1);
    }
  };

  const renderStepContent = () => {
    switch (currentStep) {
      case 0:
        return (
          <div className="gq-step-content">
            <h3>{t.profileGreeting}</h3>
            <p>{t.profileIntro}</p>

            <div className="gq-field">
              <label>{t.culturalBackground}</label>
              <SingleSelect
                options={Object.entries(t.culturalBackgrounds).map(([value, label]) => ({ value, label }))}
                value={formData.cultural_background}
                onChange={(v) => updateField('cultural_background', v)}
                name="cultural_background"
              />
            </div>

            <div className="gq-field gq-field-row">
              <div className="gq-field-half">
                <label>{t.city}</label>
                <input
                  type="text"
                  className="gq-input"
                  placeholder={t.cityPlaceholder}
                  value={formData.city}
                  onChange={(e) => updateField('city', e.target.value)}
                />
              </div>
              <div className="gq-field-half">
                <label>{t.zipCode} <span className="required">{t.required}</span></label>
                <input
                  type="text"
                  className="gq-input"
                  placeholder={t.zipCodePlaceholder}
                  value={formData.zip_code}
                  onChange={(e) => updateField('zip_code', e.target.value)}
                />
              </div>
            </div>

            <div className="gq-field">
              <label>{t.householdSize}</label>
              <SingleSelect
                options={Object.entries(t.householdSizes).map(([value, label]) => ({ value, label }))}
                value={formData.household_size}
                onChange={(v) => updateField('household_size', v)}
                name="household_size"
              />
            </div>
          </div>
        );

      case 1:
        return (
          <div className="gq-step-content">
            <h3>{t.shoppingTitle}</h3>

            <div className="gq-field">
              <label>{t.transportQuestion}</label>
              <SingleSelect
                options={Object.entries(t.transportOptions).map(([value, label]) => ({ value, label }))}
                value={formData.transport}
                onChange={(v) => updateField('transport', v)}
                name="transport"
              />
            </div>

            <div className="gq-field">
              <label>{t.shoppingPreferenceQuestion}</label>
              <SingleSelect
                options={Object.entries(t.shoppingPreferences).map(([value, label]) => ({ value, label }))}
                value={formData.shopping_preference}
                onChange={(v) => updateField('shopping_preference', v)}
                name="shopping_preference"
              />
            </div>

            <div className="gq-field">
              <label>{t.membershipsQuestion}</label>
              <MultiSelect
                options={MEMBERSHIPS}
                values={formData.memberships}
                onChange={(v) => updateField('memberships', v)}
                name="memberships"
              />
              <input
                type="text"
                className="gq-input gq-input-small"
                placeholder={t.otherMembershipPlaceholder}
                value={formData.other_membership}
                onChange={(e) => updateField('other_membership', e.target.value)}
              />
            </div>

            <div className="gq-field">
              <label>{t.storesQuestion}</label>
              <p className="gq-hint">{t.storesHint}</p>

              <div className="gq-store-section">
                <h5>{t.storeCategories.asian_online}</h5>
                <MultiSelect
                  options={STORES_ASIAN_ONLINE}
                  values={formData.preferred_stores}
                  onChange={(v) => updateField('preferred_stores', v)}
                  name="preferred_stores_asian_online"
                />
              </div>

              <div className="gq-store-section">
                <h5>{t.storeCategories.asian_physical}</h5>
                <MultiSelect
                  options={STORES_ASIAN_PHYSICAL}
                  values={formData.preferred_stores}
                  onChange={(v) => updateField('preferred_stores', v)}
                  name="preferred_stores_asian_physical"
                />
              </div>

              <div className="gq-store-section">
                <h5>{t.storeCategories.warehouse}</h5>
                <MultiSelect
                  options={STORES_WAREHOUSE}
                  values={formData.preferred_stores}
                  onChange={(v) => updateField('preferred_stores', v)}
                  name="preferred_stores_warehouse"
                />
              </div>

              <div className="gq-store-section">
                <h5>{t.storeCategories.mainstream}</h5>
                <MultiSelect
                  options={STORES_MAINSTREAM}
                  values={formData.preferred_stores}
                  onChange={(v) => updateField('preferred_stores', v)}
                  name="preferred_stores_mainstream"
                />
              </div>

              <input
                type="text"
                className="gq-input gq-input-small"
                placeholder={t.otherStorePlaceholder}
                value={formData.other_stores}
                onChange={(e) => updateField('other_stores', e.target.value)}
              />
            </div>
          </div>
        );

      case 2:
        return (
          <div className="gq-step-content">
            <h3>{t.categoriesTitle}</h3>
            <p className="gq-hint">{t.categoriesHint}</p>

            <div className="gq-field">
              <MultiSelect
                options={Object.entries(t.mainCategories).map(([value, label]) => ({ value, label }))}
                values={formData.main_categories}
                onChange={(v) => updateField('main_categories', v)}
                name="main_categories"
              />
            </div>
          </div>
        );

      case 3:
        return (
          <div className="gq-step-content">
            <h3>{t.categoryPrefsTitle}</h3>
            <p className="gq-hint">{t.categoryPrefsHint}</p>

            {formData.main_categories.includes('meat') && (
              <div className="gq-category-section">
                <h4>{t.meatTitle}</h4>
                <div className="gq-field">
                  <label>{t.meatTypeQuestion}</label>
                  <MultiSelect
                    options={Object.entries(t.meatTypes).map(([value, label]) => ({ value, label }))}
                    values={formData.meat_type}
                    onChange={(v) => updateField('meat_type', v)}
                    name="meat_type"
                  />
                </div>
                <div className="gq-field">
                  <label>{t.meatProcessingQuestion}</label>
                  <SingleSelect
                    options={Object.entries(t.meatProcessing).map(([value, label]) => ({ value, label }))}
                    value={formData.meat_processing}
                    onChange={(v) => updateField('meat_processing', v)}
                    name="meat_processing"
                  />
                </div>
                <div className="gq-field">
                  <label>{t.meatQuantityQuestion}</label>
                  <SingleSelect
                    options={Object.entries(t.meatQuantity).map(([value, label]) => ({ value, label }))}
                    value={formData.meat_quantity}
                    onChange={(v) => updateField('meat_quantity', v)}
                    name="meat_quantity"
                  />
                </div>
              </div>
            )}

            {formData.main_categories.includes('snacks') && (
              <div className="gq-category-section">
                <h4>{t.snacksTitle}</h4>
                <div className="gq-field">
                  <label>{t.snackFlavorQuestion}</label>
                  <MultiSelect
                    options={Object.entries(t.snackFlavors).map(([value, label]) => ({ value, label }))}
                    values={formData.snack_flavor}
                    onChange={(v) => updateField('snack_flavor', v)}
                    name="snack_flavor"
                  />
                </div>
                <div className="gq-field">
                  <label>{t.snackBrandsLikeLabel}</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder={t.snackBrandsLikePlaceholder}
                    value={formData.snack_brands_like}
                    onChange={(e) => updateField('snack_brands_like', e.target.value)}
                  />
                </div>
                <div className="gq-field">
                  <label>{t.snackBrandsAvoidLabel}</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder={t.snackBrandsAvoidPlaceholder}
                    value={formData.snack_brands_avoid}
                    onChange={(e) => updateField('snack_brands_avoid', e.target.value)}
                  />
                </div>
              </div>
            )}

            {formData.main_categories.includes('vegetables') && (
              <div className="gq-category-section">
                <h4>{t.vegetablesTitle}</h4>
                <div className="gq-field">
                  <label>{t.vegetableTypesLabel}</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder={t.vegetableTypesPlaceholder}
                    value={formData.vegetable_types}
                    onChange={(e) => updateField('vegetable_types', e.target.value)}
                  />
                </div>
                <div className="gq-field">
                  <label>{t.vegetableOrganicQuestion}</label>
                  <SingleSelect
                    options={Object.entries(t.vegetableOrganic).map(([value, label]) => ({ value, label }))}
                    value={formData.vegetable_organic}
                    onChange={(v) => updateField('vegetable_organic', v)}
                    name="vegetable_organic"
                  />
                </div>
              </div>
            )}

            {formData.main_categories.includes('hotpot') && (
              <div className="gq-category-section">
                <h4>{t.hotpotTitle}</h4>
                <div className="gq-field">
                  <label>{t.hotpotFrequencyQuestion}</label>
                  <SingleSelect
                    options={Object.entries(t.hotpotFrequency).map(([value, label]) => ({ value, label }))}
                    value={formData.hotpot_frequency}
                    onChange={(v) => updateField('hotpot_frequency', v)}
                    name="hotpot_frequency"
                  />
                </div>
                <div className="gq-field">
                  <label>{t.hotpotBaseQuestion}</label>
                  <MultiSelect
                    options={Object.entries(t.hotpotBases).map(([value, label]) => ({ value, label }))}
                    values={formData.hotpot_base}
                    onChange={(v) => updateField('hotpot_base', v)}
                    name="hotpot_base"
                  />
                </div>
                <div className="gq-field">
                  <label>{t.hotpotBrandsLabel}</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder={t.hotpotBrandsPlaceholder}
                    value={formData.hotpot_brands}
                    onChange={(e) => updateField('hotpot_brands', e.target.value)}
                  />
                </div>
              </div>
            )}

            {formData.main_categories.length === 0 && (
              <p className="gq-empty-hint">{t.emptyCategoryHint}</p>
            )}
          </div>
        );

      case 4:
        return (
          <div className="gq-step-content">
            <h3>{t.tasteTitle}</h3>

            <div className="gq-field">
              <label>{t.sweetnessLabel}</label>
              <RangeSlider
                value={formData.sweetness}
                onChange={(v) => updateField('sweetness', v)}
                labels={t.sweetnessLevels}
              />
            </div>

            <div className="gq-field">
              <label>{t.americanSweetsQuestion}</label>
              <SingleSelect
                options={Object.entries(t.americanSweets).map(([value, label]) => ({ value, label }))}
                value={formData.american_sweets_opinion}
                onChange={(v) => updateField('american_sweets_opinion', v)}
                name="american_sweets_opinion"
              />
            </div>

            <div className="gq-field">
              <label>{t.spicyLabel}</label>
              <RangeSlider
                value={formData.spiciness}
                onChange={(v) => updateField('spiciness', v)}
                labels={t.spicyLevels}
              />
            </div>

            <div className="gq-field">
              <label>{t.saltinessLabel}</label>
              <SingleSelect
                options={Object.entries(t.saltiness).map(([value, label]) => ({ value, label }))}
                value={formData.saltiness}
                onChange={(v) => updateField('saltiness', v)}
                name="saltiness"
              />
            </div>

            <div className="gq-field">
              <label>{t.avoidFoodsLabel}</label>
              <textarea
                className="gq-textarea"
                placeholder={t.avoidFoodsPlaceholder}
                value={formData.avoid_foods}
                onChange={(e) => updateField('avoid_foods', e.target.value)}
                rows={3}
              />
            </div>

            <div className="gq-field">
              <label>{t.dietaryLabel}</label>
              <MultiSelect
                options={Object.entries(t.dietary).map(([value, label]) => ({ value, label }))}
                values={formData.dietary_restrictions}
                onChange={(v) => updateField('dietary_restrictions', v)}
                name="dietary_restrictions"
              />
              <input
                type="text"
                className="gq-input gq-input-small"
                placeholder={t.otherDietaryPlaceholder}
                value={formData.other_dietary}
                onChange={(e) => updateField('other_dietary', e.target.value)}
              />
            </div>
          </div>
        );

      case 5:
        return (
          <div className="gq-step-content">
            <h3>{t.budgetTitle}</h3>

            <div className="gq-field">
              <label>{t.budgetQuestion}</label>
              <SingleSelect
                options={Object.entries(t.budgetMindsets).map(([value, label]) => ({ value, label }))}
                value={formData.budget_mindset}
                onChange={(v) => updateField('budget_mindset', v)}
                name="budget_mindset"
              />
            </div>

            <div className="gq-field">
              <label>{t.priorityLabel}</label>
              <DraggablePriorityList
                items={Object.entries(t.priorities).map(([value, label]) => ({ value, label }))}
                order={formData.priority_order}
                onChange={(v) => updateField('priority_order', v)}
                hint={t.priorityHint}
              />
            </div>
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <div className="gq-container">
      <div className="gq-header">
        <h2>{t.title}</h2>
        <p className="gq-subtitle">{t.subtitle}</p>
      </div>

      <ProgressBar currentStep={currentStep} totalSteps={STEP_IDS.length} />

      <div className="gq-step-indicator">
        {STEP_IDS.map((stepId, idx) => (
          <div
            key={stepId}
            className={`gq-step-dot ${idx === currentStep ? 'active' : ''} ${idx < currentStep ? 'completed' : ''}`}
            title={t.steps[stepId]}
          />
        ))}
      </div>

      <div className="gq-body">{renderStepContent()}</div>

      <div className="gq-footer">
        <button
          type="button"
          className="gq-btn gq-btn-secondary"
          onClick={currentStep === 0 ? onCancel : handleBack}
        >
          {currentStep === 0 ? t.cancel : t.back}
        </button>
        <button
          type="button"
          className="gq-btn gq-btn-primary"
          onClick={handleNext}
          disabled={!canProceed}
        >
          {currentStep === STEP_IDS.length - 1 ? t.complete : t.next}
        </button>
      </div>
    </div>
  );
}

export default GroceryPreferencesQuestionnaire;
