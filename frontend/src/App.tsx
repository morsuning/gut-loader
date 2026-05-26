import { Toaster } from "sonner";

import { HomePage } from "@/pages/HomePage";

function App() {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <HomePage />
      <Toaster position="top-right" richColors closeButton />
    </div>
  );
}

export default App;
