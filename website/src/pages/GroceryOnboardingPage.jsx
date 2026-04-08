import { useCallback, useState, useEffect } from 'react';
import { useSearchParams, useNavigate } from 'react-router-dom';
import GroceryPreferencesQuestionnaire from '../components/intake/GroceryPreferencesQuestionnaire';

const API_BASE = import.meta.env.VITE_API_BASE || '';

// Detect browser language
function detectLocale() {
  const browserLang = navigator.language || navigator.userLanguage;
  if (browserLang.startsWith('zh')) return 'zh-CN';
  return 'en-US';
}

function GroceryOnboardingPage() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(false);
  const [initialData, setInitialData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [locale, setLocale] = useState(() => {
    // Check URL param first, then browser language
    const urlLocale = searchParams.get('lang');
    if (urlLocale === 'en') return 'en-US';
    if (urlLocale === 'zh') return 'zh-CN';
    return detectLocale();
  });

  const userId = searchParams.get('user_id');
  const accountId = searchParams.get('account_id');

  const toggleLocale = useCallback(() => {
    setLocale(prev => prev === 'zh-CN' ? 'en-US' : 'zh-CN');
  }, []);

  // Load existing preferences if any
  useEffect(() => {
    if (!userId && !accountId) {
      setLoading(false);
      return;
    }

    const loadPreferences = async () => {
      try {
        const params = new URLSearchParams();
        if (userId) params.set('user_id', userId);
        if (accountId) params.set('account_id', accountId);

        const res = await fetch(`${API_BASE}/api/grocery/preferences?${params}`);
        if (res.ok) {
          const data = await res.json();
          if (data.preferences) {
            setInitialData(data.preferences);
          }
        }
      } catch (e) {
        console.warn('Failed to load existing preferences:', e);
      } finally {
        setLoading(false);
      }
    };

    loadPreferences();
  }, [userId, accountId]);

  const handleComplete = useCallback(async (formData) => {
    setSubmitting(true);
    setError(null);

    try {
      const payload = {
        user_id: userId,
        account_id: accountId,
        preferences: formData,
        subscribe_weekly: true
      };

      const res = await fetch(`${API_BASE}/api/grocery/preferences`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });

      if (!res.ok) {
        const errData = await res.json().catch(() => ({}));
        throw new Error(errData.error || `Failed to save preferences (${res.status})`);
      }

      setSuccess(true);
    } catch (e) {
      setError(e.message);
    } finally {
      setSubmitting(false);
    }
  }, [userId, accountId]);

  const handleCancel = useCallback(() => {
    navigate('/');
  }, [navigate]);

  if (loading) {
    return (
      <div className="grocery-onboarding-page">
        <div className="loading-container">
          <p>Loading...</p>
        </div>
      </div>
    );
  }

  if (success) {
    return (
      <div className="grocery-onboarding-page">
        <div className="success-container">
          <div className="success-icon">&#10003;</div>
          <h2>Preferences Saved!</h2>
          <p>You will receive personalized grocery recommendations every Sunday.</p>
          <p>You can also email us anytime with questions like:</p>
          <ul>
            <li>"Where can I find the cheapest pork belly?"</li>
            <li>"Compare tofu prices across stores"</li>
            <li>"What's on sale at Kroger this week?"</li>
          </ul>
          <button onClick={() => navigate('/')} className="back-btn">
            Back to Home
          </button>
        </div>
        <style>{`
          .success-container {
            max-width: 500px;
            margin: 4rem auto;
            padding: 2rem;
            text-align: center;
            background: #fff;
            border-radius: 1rem;
            box-shadow: 0 4px 24px rgba(0,0,0,0.1);
          }
          .success-icon {
            width: 64px;
            height: 64px;
            margin: 0 auto 1rem;
            background: #10b981;
            color: white;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 2rem;
          }
          .success-container h2 {
            color: #111827;
            margin-bottom: 0.5rem;
          }
          .success-container p {
            color: #6b7280;
            margin-bottom: 1rem;
          }
          .success-container ul {
            text-align: left;
            background: #f9fafb;
            padding: 1rem 1rem 1rem 2rem;
            border-radius: 0.5rem;
            margin-bottom: 1.5rem;
          }
          .success-container li {
            color: #374151;
            margin-bottom: 0.5rem;
          }
          .back-btn {
            padding: 0.75rem 2rem;
            background: #6366f1;
            color: white;
            border: none;
            border-radius: 0.5rem;
            font-size: 1rem;
            cursor: pointer;
          }
          .back-btn:hover {
            background: #4f46e5;
          }
        `}</style>
      </div>
    );
  }

  return (
    <div className="grocery-onboarding-page">
      <button className="locale-toggle" onClick={toggleLocale}>
        {locale === 'zh-CN' ? 'English' : '中文'}
      </button>
      {error && (
        <div className="error-banner">
          {error}
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}
      <GroceryPreferencesQuestionnaire
        onComplete={handleComplete}
        onCancel={handleCancel}
        initialData={initialData || {}}
        locale={locale}
      />
      {submitting && (
        <div className="submitting-overlay">
          <div className="spinner" />
          <p>Saving your preferences...</p>
        </div>
      )}
      <style>{`
        .grocery-onboarding-page {
          min-height: 100vh;
          background: linear-gradient(135deg, #f5f7fa 0%, #e4e8ec 100%);
          padding: 2rem 1rem;
          position: relative;
        }
        .locale-toggle {
          position: fixed;
          bottom: 1rem;
          right: 1rem;
          padding: 0.5rem 1rem;
          background: white;
          border: 1px solid #e5e7eb;
          border-radius: 0.5rem;
          font-size: 0.875rem;
          cursor: pointer;
          color: #374151;
          transition: all 0.2s;
          z-index: 100;
          box-shadow: 0 2px 8px rgba(0,0,0,0.1);
        }
        .locale-toggle:hover {
          background: #f9fafb;
          border-color: #d1d5db;
        }
        .loading-container {
          display: flex;
          justify-content: center;
          align-items: center;
          min-height: 50vh;
          color: #6b7280;
        }
        .error-banner {
          max-width: 600px;
          margin: 0 auto 1rem;
          padding: 1rem;
          background: #fef2f2;
          border: 1px solid #fecaca;
          border-radius: 0.5rem;
          color: #dc2626;
          display: flex;
          justify-content: space-between;
          align-items: center;
        }
        .error-banner button {
          background: none;
          border: none;
          font-size: 1.25rem;
          color: #dc2626;
          cursor: pointer;
        }
        .submitting-overlay {
          position: fixed;
          inset: 0;
          background: rgba(255,255,255,0.9);
          display: flex;
          flex-direction: column;
          justify-content: center;
          align-items: center;
          z-index: 1000;
        }
        .spinner {
          width: 48px;
          height: 48px;
          border: 4px solid #e5e7eb;
          border-top-color: #6366f1;
          border-radius: 50%;
          animation: spin 1s linear infinite;
        }
        @keyframes spin {
          to { transform: rotate(360deg); }
        }
        .submitting-overlay p {
          margin-top: 1rem;
          color: #6b7280;
        }
      `}</style>
    </div>
  );
}

export default GroceryOnboardingPage;
