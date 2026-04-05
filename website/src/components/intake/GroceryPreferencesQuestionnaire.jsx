import { useState, useCallback, useMemo } from 'react';
import './GroceryPreferencesQuestionnaire.css';

const STEPS = [
  { id: 'profile', title: 'Basic Profile', titleZh: '基本信息' },
  { id: 'shopping', title: 'Shopping Habits', titleZh: '购物习惯' },
  { id: 'categories', title: 'Categories', titleZh: '常买品类' },
  { id: 'category_prefs', title: 'Category Preferences', titleZh: '品类偏好' },
  { id: 'taste', title: 'Taste Profile', titleZh: '口味偏好' },
  { id: 'budget', title: 'Budget & Priorities', titleZh: '预算与优先级' }
];

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

function DraggablePriorityList({ items, order, onChange }) {
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
      <p className="gq-hint">拖拽排序（1 = 最重要）</p>
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
  initialData = {}
}) {
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
    if (currentStep < STEPS.length - 1) {
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
            <h3>你好！我是你的智能买菜助手</h3>
            <p>为了给你更好的推荐，先了解一下你的情况</p>

            <div className="gq-field">
              <label>你的文化背景是？</label>
              <SingleSelect
                options={CULTURAL_BACKGROUNDS}
                value={formData.cultural_background}
                onChange={(v) => updateField('cultural_background', v)}
                name="cultural_background"
              />
            </div>

            <div className="gq-field gq-field-row">
              <div className="gq-field-half">
                <label>你住在哪个城市？</label>
                <input
                  type="text"
                  className="gq-input"
                  placeholder="例如：Ann Arbor, MI"
                  value={formData.city}
                  onChange={(e) => updateField('city', e.target.value)}
                />
              </div>
              <div className="gq-field-half">
                <label>Zip Code <span className="required">*</span></label>
                <input
                  type="text"
                  className="gq-input"
                  placeholder="例如：48109"
                  value={formData.zip_code}
                  onChange={(e) => updateField('zip_code', e.target.value)}
                />
              </div>
            </div>

            <div className="gq-field">
              <label>你一般是为几个人买菜？</label>
              <SingleSelect
                options={HOUSEHOLD_SIZES}
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
            <h3>购物习惯</h3>

            <div className="gq-field">
              <label>你有车吗？最远愿意开多久去买菜？</label>
              <SingleSelect
                options={TRANSPORT_OPTIONS}
                value={formData.transport}
                onChange={(v) => updateField('transport', v)}
                name="transport"
              />
            </div>

            <div className="gq-field">
              <label>你更喜欢？</label>
              <SingleSelect
                options={SHOPPING_PREFERENCES}
                value={formData.shopping_preference}
                onChange={(v) => updateField('shopping_preference', v)}
                name="shopping_preference"
              />
            </div>

            <div className="gq-field">
              <label>你有这些会员卡吗？（多选）</label>
              <MultiSelect
                options={MEMBERSHIPS}
                values={formData.memberships}
                onChange={(v) => updateField('memberships', v)}
                name="memberships"
              />
              <input
                type="text"
                className="gq-input gq-input-small"
                placeholder="其他会员卡..."
                value={formData.other_membership}
                onChange={(e) => updateField('other_membership', e.target.value)}
              />
            </div>

            <div className="gq-field">
              <label>你平时在哪些超市买菜？（多选）</label>
              <p className="gq-hint">选择你常去的超市，我们会优先比较这些店的价格</p>

              <div className="gq-store-section">
                <h5>亚洲超市 - 网购</h5>
                <MultiSelect
                  options={STORES_ASIAN_ONLINE}
                  values={formData.preferred_stores}
                  onChange={(v) => updateField('preferred_stores', v)}
                  name="preferred_stores_asian_online"
                />
              </div>

              <div className="gq-store-section">
                <h5>亚洲超市 - 实体店</h5>
                <MultiSelect
                  options={STORES_ASIAN_PHYSICAL}
                  values={formData.preferred_stores}
                  onChange={(v) => updateField('preferred_stores', v)}
                  name="preferred_stores_asian_physical"
                />
              </div>

              <div className="gq-store-section">
                <h5>仓储会员店</h5>
                <MultiSelect
                  options={STORES_WAREHOUSE}
                  values={formData.preferred_stores}
                  onChange={(v) => updateField('preferred_stores', v)}
                  name="preferred_stores_warehouse"
                />
              </div>

              <div className="gq-store-section">
                <h5>美国主流超市</h5>
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
                placeholder="其他超市（如本地华人超市）..."
                value={formData.other_stores}
                onChange={(e) => updateField('other_stores', e.target.value)}
              />
            </div>
          </div>
        );

      case 2:
        return (
          <div className="gq-step-content">
            <h3>你主要买哪些品类？</h3>
            <p className="gq-hint">选择后会针对你选的品类问更细的问题</p>

            <div className="gq-field">
              <MultiSelect
                options={MAIN_CATEGORIES}
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
            <h3>品类偏好细节</h3>
            <p className="gq-hint">根据你选择的品类，我们想了解更多细节</p>

            {formData.main_categories.includes('meat') && (
              <div className="gq-category-section">
                <h4>关于肉类</h4>
                <div className="gq-field">
                  <label>你更常买哪种肉？（多选）</label>
                  <MultiSelect
                    options={[
                      { value: 'pork', label: '猪肉' },
                      { value: 'beef', label: '牛肉' },
                      { value: 'chicken', label: '鸡肉' },
                      { value: 'lamb', label: '羊肉' }
                    ]}
                    values={formData.meat_type}
                    onChange={(v) => updateField('meat_type', v)}
                    name="meat_type"
                  />
                </div>
                <div className="gq-field">
                  <label>你会自己处理生肉吗？</label>
                  <SingleSelect
                    options={[
                      { value: 'can_process', label: '可以，没问题' },
                      { value: 'prefer_cut', label: '更喜欢买切好的' },
                      { value: 'must_cut', label: '必须切好的，不会处理' }
                    ]}
                    value={formData.meat_processing}
                    onChange={(v) => updateField('meat_processing', v)}
                    name="meat_processing"
                  />
                </div>
                <div className="gq-field">
                  <label>对肉的分量有要求吗？</label>
                  <SingleSelect
                    options={[
                      { value: 'small', label: '一次只买1-2lb' },
                      { value: 'medium', label: '可以买3-5lb' },
                      { value: 'bulk', label: '可以批量买冷冻' }
                    ]}
                    value={formData.meat_quantity}
                    onChange={(v) => updateField('meat_quantity', v)}
                    name="meat_quantity"
                  />
                </div>
              </div>
            )}

            {formData.main_categories.includes('snacks') && (
              <div className="gq-category-section">
                <h4>关于零食</h4>
                <div className="gq-field">
                  <label>口味偏好？（多选）</label>
                  <MultiSelect
                    options={[
                      { value: 'salty', label: '咸口' },
                      { value: 'sweet', label: '甜口' },
                      { value: 'spicy', label: '辣口' }
                    ]}
                    values={formData.snack_flavor}
                    onChange={(v) => updateField('snack_flavor', v)}
                    name="snack_flavor"
                  />
                </div>
                <div className="gq-field">
                  <label>喜欢的品牌？</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder="例如：旺旺、乐事、百草味"
                    value={formData.snack_brands_like}
                    onChange={(e) => updateField('snack_brands_like', e.target.value)}
                  />
                </div>
                <div className="gq-field">
                  <label>排斥的品牌/口味？</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder="例如：美式糖果"
                    value={formData.snack_brands_avoid}
                    onChange={(e) => updateField('snack_brands_avoid', e.target.value)}
                  />
                </div>
              </div>
            )}

            {formData.main_categories.includes('vegetables') && (
              <div className="gq-category-section">
                <h4>关于蔬菜</h4>
                <div className="gq-field">
                  <label>你常买哪些蔬菜？</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder="例如：韭菜、空心菜、小白菜"
                    value={formData.vegetable_types}
                    onChange={(e) => updateField('vegetable_types', e.target.value)}
                  />
                </div>
                <div className="gq-field">
                  <label>有机蔬菜重要吗？</label>
                  <SingleSelect
                    options={[
                      { value: 'important', label: '很重要' },
                      { value: 'nice_to_have', label: '有更好' },
                      { value: 'not_important', label: '不重要' }
                    ]}
                    value={formData.vegetable_organic}
                    onChange={(v) => updateField('vegetable_organic', v)}
                    name="vegetable_organic"
                  />
                </div>
              </div>
            )}

            {formData.main_categories.includes('hotpot') && (
              <div className="gq-category-section">
                <h4>关于火锅</h4>
                <div className="gq-field">
                  <label>多久吃一次火锅？</label>
                  <SingleSelect
                    options={[
                      { value: 'weekly', label: '每周' },
                      { value: 'biweekly', label: '每两周' },
                      { value: 'monthly', label: '每月1-2次' },
                      { value: 'rarely', label: '偶尔' }
                    ]}
                    value={formData.hotpot_frequency}
                    onChange={(v) => updateField('hotpot_frequency', v)}
                    name="hotpot_frequency"
                  />
                </div>
                <div className="gq-field">
                  <label>喜欢什么锅底？（多选）</label>
                  <MultiSelect
                    options={[
                      { value: 'spicy', label: '麻辣' },
                      { value: 'clear', label: '清汤' },
                      { value: 'tomato', label: '番茄' },
                      { value: 'mushroom', label: '菌菇' }
                    ]}
                    values={formData.hotpot_base}
                    onChange={(v) => updateField('hotpot_base', v)}
                    name="hotpot_base"
                  />
                </div>
                <div className="gq-field">
                  <label>喜欢的火锅品牌？</label>
                  <input
                    type="text"
                    className="gq-input"
                    placeholder="例如：海底捞、小龙坎、德庄"
                    value={formData.hotpot_brands}
                    onChange={(e) => updateField('hotpot_brands', e.target.value)}
                  />
                </div>
              </div>
            )}

            {formData.main_categories.length === 0 && (
              <p className="gq-empty-hint">请先在上一步选择你常买的品类</p>
            )}
          </div>
        );

      case 4:
        return (
          <div className="gq-step-content">
            <h3>口味偏好</h3>

            <div className="gq-field">
              <label>甜度接受度</label>
              <RangeSlider
                value={formData.sweetness}
                onChange={(v) => updateField('sweetness', v)}
                labels={['很淡', '偏淡', '适中', '偏甜', '很甜']}
              />
            </div>

            <div className="gq-field">
              <label>美式甜品对你来说通常：</label>
              <SingleSelect
                options={[
                  { value: 'too_sweet', label: '太甜' },
                  { value: 'just_right', label: '刚好' },
                  { value: 'could_be_sweeter', label: '可以更甜' }
                ]}
                value={formData.american_sweets_opinion}
                onChange={(v) => updateField('american_sweets_opinion', v)}
                name="american_sweets_opinion"
              />
            </div>

            <div className="gq-field">
              <label>辣度接受度</label>
              <RangeSlider
                value={formData.spiciness}
                onChange={(v) => updateField('spiciness', v)}
                labels={['不能吃辣', '微辣', '中辣', '辣', '越辣越好']}
              />
            </div>

            <div className="gq-field">
              <label>咸度偏好</label>
              <SingleSelect
                options={[
                  { value: 'light', label: '偏淡' },
                  { value: 'normal', label: '正常' },
                  { value: 'salty', label: '偏咸' }
                ]}
                value={formData.saltiness}
                onChange={(v) => updateField('saltiness', v)}
                name="saltiness"
              />
            </div>

            <div className="gq-field">
              <label>有什么特别排斥的食物/品牌吗？</label>
              <textarea
                className="gq-textarea"
                placeholder="例如：Kraft芝士、美式糖果、某些调味品等"
                value={formData.avoid_foods}
                onChange={(e) => updateField('avoid_foods', e.target.value)}
                rows={3}
              />
            </div>

            <div className="gq-field">
              <label>有饮食限制吗？（多选）</label>
              <MultiSelect
                options={DIETARY_RESTRICTIONS}
                values={formData.dietary_restrictions}
                onChange={(v) => updateField('dietary_restrictions', v)}
                name="dietary_restrictions"
              />
              <input
                type="text"
                className="gq-input gq-input-small"
                placeholder="其他过敏/限制..."
                value={formData.other_dietary}
                onChange={(e) => updateField('other_dietary', e.target.value)}
              />
            </div>
          </div>
        );

      case 5:
        return (
          <div className="gq-step-content">
            <h3>预算与优先级</h3>

            <div className="gq-field">
              <label>买菜预算大概是？</label>
              <SingleSelect
                options={BUDGET_MINDSETS}
                value={formData.budget_mindset}
                onChange={(v) => updateField('budget_mindset', v)}
                name="budget_mindset"
              />
            </div>

            <div className="gq-field">
              <label>以下因素对你的重要程度排序：</label>
              <DraggablePriorityList
                items={PRIORITY_FACTORS}
                order={formData.priority_order}
                onChange={(v) => updateField('priority_order', v)}
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
        <h2>智能买菜助手</h2>
        <p className="gq-subtitle">Smart Grocery Preferences</p>
      </div>

      <ProgressBar currentStep={currentStep} totalSteps={STEPS.length} />

      <div className="gq-step-indicator">
        {STEPS.map((step, idx) => (
          <div
            key={step.id}
            className={`gq-step-dot ${idx === currentStep ? 'active' : ''} ${idx < currentStep ? 'completed' : ''}`}
            title={step.titleZh}
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
          {currentStep === 0 ? '取消' : '上一步'}
        </button>
        <button
          type="button"
          className="gq-btn gq-btn-primary"
          onClick={handleNext}
          disabled={!canProceed}
        >
          {currentStep === STEPS.length - 1 ? '完成' : '下一步'}
        </button>
      </div>
    </div>
  );
}

export default GroceryPreferencesQuestionnaire;
