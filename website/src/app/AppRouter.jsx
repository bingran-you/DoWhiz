import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import LandingPage from '../pages/LandingPage';
import StartupIntakePage from '../pages/StartupIntakePage';
import WorkspaceHomePage from '../pages/WorkspaceHomePage';
import DashboardPage from '../pages/internal/DashboardPage';
import GroceryOnboardingPage from '../pages/GroceryOnboardingPage';

function AppRouter() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<LandingPage locale="en-US" />} />
        <Route path="/cn" element={<LandingPage locale="zh-CN" />} />
        <Route path="/cn/*" element={<LandingPage locale="zh-CN" />} />
        <Route path="/start" element={<StartupIntakePage />} />
        <Route path="/workspace" element={<WorkspaceHomePage />} />
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/grocery/onboarding" element={<GroceryOnboardingPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}

export default AppRouter;
