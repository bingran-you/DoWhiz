// Grocery questionnaire translations
// Supports: zh-CN (Chinese), en-US (English)

export const translations = {
  'zh-CN': {
    // Header
    title: '智能买菜助手',
    subtitle: 'Smart Grocery Preferences',

    // Steps
    steps: {
      profile: '基本信息',
      shopping: '购物习惯',
      categories: '常买品类',
      category_prefs: '品类偏好',
      taste: '口味偏好',
      budget: '预算与优先级'
    },

    // Buttons
    cancel: '取消',
    back: '上一步',
    next: '下一步',
    complete: '完成',

    // Step 1: Profile
    profileGreeting: '你好！我是你的智能买菜助手',
    profileIntro: '为了给你更好的推荐，先了解一下你的情况',
    culturalBackground: '你的文化背景是？',
    city: '你住在哪个城市？',
    cityPlaceholder: '例如：Ann Arbor, MI',
    zipCode: 'Zip Code',
    zipCodePlaceholder: '例如：48109',
    householdSize: '你一般是为几个人买菜？',
    required: '*',

    // Cultural backgrounds
    culturalBackgrounds: {
      mainland_china: '中国大陆',
      taiwan_hk_macau: '台湾/港澳',
      abc: 'ABC/华裔美国人',
      korea_japan: '韩国/日本',
      southeast_asia: '东南亚',
      other_asian: '其他亚裔',
      non_asian: '非亚裔但喜欢亚洲食品'
    },

    // Household sizes
    householdSizes: {
      '1': '1人（自己）',
      '2': '2人（情侣/室友）',
      '3-4': '3-4人（小家庭）',
      '5+': '5人以上（大家庭）'
    },

    // Step 2: Shopping
    shoppingTitle: '购物习惯',
    transportQuestion: '你有车吗？最远愿意开多久去买菜？',
    shoppingPreferenceQuestion: '你更喜欢？',
    membershipsQuestion: '你有这些会员卡吗？（多选）',
    otherMembershipPlaceholder: '其他会员卡...',
    storesQuestion: '你平时在哪些超市买菜？（多选）',
    storesHint: '选择你常去的超市，我们会优先比较这些店的价格',
    otherStorePlaceholder: '其他超市（如本地华人超市）...',

    // Transport options
    transportOptions: {
      no_car: '没有车，靠公共交通/走路',
      car_15min: '有车，15分钟内',
      car_30min: '有车，30分钟内',
      car_60min: '有车，1小时内也可以'
    },

    // Shopping preferences
    shoppingPreferences: {
      delivery: '网购送货（Weee/Instacart）',
      in_store: '线下超市',
      both: '两者都可以'
    },

    // Store categories
    storeCategories: {
      asian_online: '亚洲超市 - 网购',
      asian_physical: '亚洲超市 - 实体店',
      warehouse: '仓储会员店',
      mainstream: '美国主流超市'
    },

    // Step 3: Categories
    categoriesTitle: '你主要买哪些品类？',
    categoriesHint: '选择后会针对你选的品类问更细的问题',

    // Main categories
    mainCategories: {
      meat: '肉类（猪/牛/羊/鸡）',
      seafood: '海鲜',
      vegetables: '蔬菜',
      fruits: '水果',
      snacks: '零食饮料',
      hotpot: '火锅/烧烤食材',
      condiments: '调味料/酱料',
      instant: '速食/方便面',
      dairy: '奶制品/鸡蛋',
      bakery: '面包/烘焙'
    },

    // Step 4: Category Preferences
    categoryPrefsTitle: '品类偏好细节',
    categoryPrefsHint: '根据你选择的品类，我们想了解更多细节',
    emptyCategoryHint: '请先在上一步选择你常买的品类',

    // Meat section
    meatTitle: '关于肉类',
    meatTypeQuestion: '你更常买哪种肉？（多选）',
    meatTypes: { pork: '猪肉', beef: '牛肉', chicken: '鸡肉', lamb: '羊肉' },
    meatProcessingQuestion: '你会自己处理生肉吗？',
    meatProcessing: {
      can_process: '可以，没问题',
      prefer_cut: '更喜欢买切好的',
      must_cut: '必须切好的，不会处理'
    },
    meatQuantityQuestion: '对肉的分量有要求吗？',
    meatQuantity: {
      small: '一次只买1-2lb',
      medium: '可以买3-5lb',
      bulk: '可以批量买冷冻'
    },

    // Snacks section
    snacksTitle: '关于零食',
    snackFlavorQuestion: '口味偏好？（多选）',
    snackFlavors: { salty: '咸口', sweet: '甜口', spicy: '辣口' },
    snackBrandsLikeLabel: '喜欢的品牌？',
    snackBrandsLikePlaceholder: '例如：旺旺、乐事、百草味',
    snackBrandsAvoidLabel: '排斥的品牌/口味？',
    snackBrandsAvoidPlaceholder: '例如：美式糖果',

    // Vegetables section
    vegetablesTitle: '关于蔬菜',
    vegetableTypesLabel: '你常买哪些蔬菜？',
    vegetableTypesPlaceholder: '例如：韭菜、空心菜、小白菜',
    vegetableOrganicQuestion: '有机蔬菜重要吗？',
    vegetableOrganic: {
      important: '很重要',
      nice_to_have: '有更好',
      not_important: '不重要'
    },

    // Hotpot section
    hotpotTitle: '关于火锅',
    hotpotFrequencyQuestion: '多久吃一次火锅？',
    hotpotFrequency: {
      weekly: '每周',
      biweekly: '每两周',
      monthly: '每月1-2次',
      rarely: '偶尔'
    },
    hotpotBaseQuestion: '喜欢什么锅底？（多选）',
    hotpotBases: { spicy: '麻辣', clear: '清汤', tomato: '番茄', mushroom: '菌菇' },
    hotpotBrandsLabel: '喜欢的火锅品牌？',
    hotpotBrandsPlaceholder: '例如：海底捞、小龙坎、德庄',

    // Step 5: Taste
    tasteTitle: '口味偏好',
    sweetnessLabel: '甜度接受度',
    sweetnessLevels: ['很淡', '偏淡', '适中', '偏甜', '很甜'],
    americanSweetsQuestion: '美式甜品对你来说通常：',
    americanSweets: {
      too_sweet: '太甜',
      just_right: '刚好',
      could_be_sweeter: '可以更甜'
    },
    spicyLabel: '辣度接受度',
    spicyLevels: ['不能吃辣', '微辣', '中辣', '辣', '越辣越好'],
    saltinessLabel: '咸度偏好',
    saltiness: { light: '偏淡', normal: '正常', salty: '偏咸' },
    avoidFoodsLabel: '有什么特别排斥的食物/品牌吗？',
    avoidFoodsPlaceholder: '例如：Kraft芝士、美式糖果、某些调味品等',
    dietaryLabel: '有饮食限制吗？（多选）',
    dietary: {
      none: '无',
      vegetarian: '素食/纯素',
      no_pork: '不吃猪肉（宗教原因）',
      lactose: '乳糖不耐受',
      gluten: '麸质过敏'
    },
    otherDietaryPlaceholder: '其他过敏/限制...',

    // Step 6: Budget
    budgetTitle: '预算与优先级',
    budgetQuestion: '买菜预算大概是？',
    budgetMindsets: {
      price_first: '能省则省，价格最重要',
      value: '性价比优先，质量也要看',
      quality_first: '质量优先，价格其次',
      no_concern: '不太在意价格'
    },
    priorityLabel: '以下因素对你的重要程度排序：',
    priorityHint: '拖拽排序（1 = 最重要）',
    priorities: {
      price: '价格便宜',
      quality: '产品新鲜/质量好',
      convenience: '距离近/方便',
      variety: '品种齐全'
    }
  },

  'en-US': {
    // Header
    title: 'Smart Grocery Assistant',
    subtitle: 'Grocery Preferences',

    // Steps
    steps: {
      profile: 'Basic Profile',
      shopping: 'Shopping Habits',
      categories: 'Categories',
      category_prefs: 'Preferences',
      taste: 'Taste Profile',
      budget: 'Budget & Priorities'
    },

    // Buttons
    cancel: 'Cancel',
    back: 'Back',
    next: 'Next',
    complete: 'Complete',

    // Step 1: Profile
    profileGreeting: 'Hi! I\'m your smart grocery assistant',
    profileIntro: 'To give you better recommendations, let me learn about you',
    culturalBackground: 'What\'s your cultural background?',
    city: 'What city do you live in?',
    cityPlaceholder: 'e.g., Ann Arbor, MI',
    zipCode: 'Zip Code',
    zipCodePlaceholder: 'e.g., 48109',
    householdSize: 'How many people do you usually shop for?',
    required: '*',

    // Cultural backgrounds
    culturalBackgrounds: {
      mainland_china: 'Mainland China',
      taiwan_hk_macau: 'Taiwan/Hong Kong/Macau',
      abc: 'ABC/Chinese American',
      korea_japan: 'Korea/Japan',
      southeast_asia: 'Southeast Asia',
      other_asian: 'Other Asian',
      non_asian: 'Non-Asian but loves Asian food'
    },

    // Household sizes
    householdSizes: {
      '1': '1 person (just me)',
      '2': '2 people (couple/roommate)',
      '3-4': '3-4 people (small family)',
      '5+': '5+ people (large family)'
    },

    // Step 2: Shopping
    shoppingTitle: 'Shopping Habits',
    transportQuestion: 'Do you have a car? How far would you drive to shop?',
    shoppingPreferenceQuestion: 'Do you prefer?',
    membershipsQuestion: 'Do you have these memberships? (select all)',
    otherMembershipPlaceholder: 'Other memberships...',
    storesQuestion: 'Where do you usually shop? (select all)',
    storesHint: 'Select your regular stores and we\'ll prioritize comparing their prices',
    otherStorePlaceholder: 'Other stores (e.g., local Asian market)...',

    // Transport options
    transportOptions: {
      no_car: 'No car, public transit/walking',
      car_15min: 'Have car, within 15 min',
      car_30min: 'Have car, within 30 min',
      car_60min: 'Have car, up to 1 hour is fine'
    },

    // Shopping preferences
    shoppingPreferences: {
      delivery: 'Online delivery (Weee/Instacart)',
      in_store: 'In-store shopping',
      both: 'Both are fine'
    },

    // Store categories
    storeCategories: {
      asian_online: 'Asian Grocery - Online',
      asian_physical: 'Asian Grocery - In-store',
      warehouse: 'Warehouse Clubs',
      mainstream: 'Mainstream US Stores'
    },

    // Step 3: Categories
    categoriesTitle: 'What categories do you mainly buy?',
    categoriesHint: 'We\'ll ask more detailed questions based on your selection',

    // Main categories
    mainCategories: {
      meat: 'Meat (pork/beef/lamb/chicken)',
      seafood: 'Seafood',
      vegetables: 'Vegetables',
      fruits: 'Fruits',
      snacks: 'Snacks & Drinks',
      hotpot: 'Hotpot/BBQ ingredients',
      condiments: 'Condiments/Sauces',
      instant: 'Instant food/Noodles',
      dairy: 'Dairy/Eggs',
      bakery: 'Bakery'
    },

    // Step 4: Category Preferences
    categoryPrefsTitle: 'Category Preferences',
    categoryPrefsHint: 'Based on your selected categories, we\'d like to know more details',
    emptyCategoryHint: 'Please select categories in the previous step first',

    // Meat section
    meatTitle: 'About Meat',
    meatTypeQuestion: 'Which meats do you buy most often? (select all)',
    meatTypes: { pork: 'Pork', beef: 'Beef', chicken: 'Chicken', lamb: 'Lamb' },
    meatProcessingQuestion: 'Can you process raw meat yourself?',
    meatProcessing: {
      can_process: 'Yes, no problem',
      prefer_cut: 'Prefer pre-cut',
      must_cut: 'Must be pre-cut, can\'t process'
    },
    meatQuantityQuestion: 'Any preference on portion size?',
    meatQuantity: {
      small: 'Only 1-2 lbs at a time',
      medium: 'Can buy 3-5 lbs',
      bulk: 'Can buy bulk frozen'
    },

    // Snacks section
    snacksTitle: 'About Snacks',
    snackFlavorQuestion: 'Flavor preferences? (select all)',
    snackFlavors: { salty: 'Salty', sweet: 'Sweet', spicy: 'Spicy' },
    snackBrandsLikeLabel: 'Favorite brands?',
    snackBrandsLikePlaceholder: 'e.g., Want Want, Lay\'s, Baicaowei',
    snackBrandsAvoidLabel: 'Brands/flavors you avoid?',
    snackBrandsAvoidPlaceholder: 'e.g., American candy',

    // Vegetables section
    vegetablesTitle: 'About Vegetables',
    vegetableTypesLabel: 'What vegetables do you usually buy?',
    vegetableTypesPlaceholder: 'e.g., chives, water spinach, bok choy',
    vegetableOrganicQuestion: 'How important is organic?',
    vegetableOrganic: {
      important: 'Very important',
      nice_to_have: 'Nice to have',
      not_important: 'Not important'
    },

    // Hotpot section
    hotpotTitle: 'About Hotpot',
    hotpotFrequencyQuestion: 'How often do you have hotpot?',
    hotpotFrequency: {
      weekly: 'Weekly',
      biweekly: 'Every two weeks',
      monthly: '1-2 times a month',
      rarely: 'Occasionally'
    },
    hotpotBaseQuestion: 'What soup bases do you like? (select all)',
    hotpotBases: { spicy: 'Spicy/Mala', clear: 'Clear broth', tomato: 'Tomato', mushroom: 'Mushroom' },
    hotpotBrandsLabel: 'Favorite hotpot brands?',
    hotpotBrandsPlaceholder: 'e.g., Haidilao, Xiaolongkan, Dezhuang',

    // Step 5: Taste
    tasteTitle: 'Taste Profile',
    sweetnessLabel: 'Sweetness tolerance',
    sweetnessLevels: ['Very light', 'Light', 'Medium', 'Sweet', 'Very sweet'],
    americanSweetsQuestion: 'American desserts usually taste:',
    americanSweets: {
      too_sweet: 'Too sweet',
      just_right: 'Just right',
      could_be_sweeter: 'Could be sweeter'
    },
    spicyLabel: 'Spice tolerance',
    spicyLevels: ['No spice', 'Mild', 'Medium', 'Spicy', 'The spicier the better'],
    saltinessLabel: 'Salt preference',
    saltiness: { light: 'Light', normal: 'Normal', salty: 'Salty' },
    avoidFoodsLabel: 'Any foods/brands you particularly avoid?',
    avoidFoodsPlaceholder: 'e.g., Kraft cheese, American candy, certain condiments',
    dietaryLabel: 'Any dietary restrictions? (select all)',
    dietary: {
      none: 'None',
      vegetarian: 'Vegetarian/Vegan',
      no_pork: 'No pork (religious)',
      lactose: 'Lactose intolerant',
      gluten: 'Gluten-free'
    },
    otherDietaryPlaceholder: 'Other allergies/restrictions...',

    // Step 6: Budget
    budgetTitle: 'Budget & Priorities',
    budgetQuestion: 'What\'s your grocery budget mindset?',
    budgetMindsets: {
      price_first: 'Save as much as possible, price is most important',
      value: 'Value for money, but quality matters too',
      quality_first: 'Quality first, price second',
      no_concern: 'Price is not a concern'
    },
    priorityLabel: 'Rank these factors by importance:',
    priorityHint: 'Drag to reorder (1 = most important)',
    priorities: {
      price: 'Low price',
      quality: 'Fresh/Quality products',
      convenience: 'Close/Convenient',
      variety: 'Wide variety'
    }
  }
};

// Helper to get translation
export function useTranslation(locale = 'zh-CN') {
  const t = translations[locale] || translations['zh-CN'];
  return {
    t,
    locale,
    // Helper for nested translations
    get: (key) => {
      const keys = key.split('.');
      let result = t;
      for (const k of keys) {
        result = result?.[k];
      }
      return result || key;
    }
  };
}

export default translations;
