#!/usr/bin/env python3
"""
Generic AI Agent that integrates with Deko
Demonstrates action submission, polling for verdicts, and execution
"""

import requests
import json
import time
import sys
from typing import Optional, Dict, Any
from dataclasses import dataclass


@dataclass
class DekoConfig:
    deko_url: str = "http://localhost:8000"
    admin_password: str = "changeme"
    api_key: Optional[str] = None


class DekoAgent:
    """Agent that submits actions to Deko for approval before execution"""

    def __init__(self, agent_name: str, config: DekoConfig):
        self.agent_name = agent_name
        self.config = config
        self.api_key = config.api_key
        self.headers = {
            "Content-Type": "application/json"
        }
        
        if self.api_key:
            self.headers["X-API-Key"] = self.api_key

    def register(self) -> bool:
        """Register agent with Deko and get API key"""
        print(f"\n📝 Registering agent '{self.agent_name}' with Deko...")
        
        try:
            response = requests.post(
                f"{self.config.deko_url}/auth/register",
                json={"name": self.agent_name},
                headers={
                    "X-Admin-Password": self.config.admin_password,
                    "Content-Type": "application/json"
                },
                timeout=10
            )
            
            if response.status_code == 201 or response.status_code == 200:
                data = response.json()
                self.api_key = data.get("api_key")
                self.headers["X-API-Key"] = self.api_key
                
                print(f"✅ Agent registered successfully!")
                print(f"   Agent ID: {data.get('agent_id')}")
                print(f"   API Key: {self.api_key[:20]}...")
                return True
            else:
                print(f"❌ Registration failed: {response.status_code}")
                print(f"   Response: {response.text}")
                return False
                
        except Exception as e:
            print(f"❌ Registration error: {e}")
            return False

    def submit_action(self, intent: str, payload: Dict[str, Any], 
                     target_url: str, screenshot: Optional[str] = None) -> Optional[str]:
        """Submit action to Deko for approval"""
        print(f"\n🔄 Submitting action to Deko...")
        print(f"   Intent: {intent}")
        
        try:
            data = {
                "intent": intent,
                "payload": payload,
                "target_url": target_url
            }
            
            if screenshot:
                data["screenshot_base64"] = screenshot
            
            response = requests.post(
                f"{self.config.deko_url}/action",
                json=data,
                headers=self.headers,
                timeout=10
            )
            
            if response.status_code == 201 or response.status_code == 200:
                data = response.json()
                action_id = data.get("id")
                print(f"✅ Action submitted!")
                print(f"   Action ID: {action_id}")
                return action_id
            else:
                print(f"❌ Submission failed: {response.status_code}")
                print(f"   Response: {response.text}")
                return None
                
        except Exception as e:
            print(f"❌ Submission error: {e}")
            return None

    def poll_verdict(self, action_id: str, max_wait: int = 30, poll_interval: int = 2) -> Optional[Dict]:
        """Poll Deko for action verdict"""
        print(f"\n⏳ Waiting for Deko's verdict (max {max_wait}s)...")
        
        start_time = time.time()
        
        while time.time() - start_time < max_wait:
            try:
                response = requests.get(
                    f"{self.config.deko_url}/action/{action_id}/status",
                    headers=self.headers,
                    timeout=10
                )
                
                if response.status_code == 200:
                    verdict = response.json()
                    
                    if verdict.get("status") != "pending":
                        # Got a verdict
                        decision = verdict.get("decision", "unknown")
                        reason = verdict.get("reason", "")
                        risk_level = verdict.get("risk_level", "")
                        
                        if decision == "approved":
                            print(f"✅ APPROVED")
                            print(f"   Risk Level: {risk_level}")
                            if reason:
                                print(f"   Reason: {reason}")
                        elif decision == "denied":
                            print(f"❌ DENIED")
                            print(f"   Risk Level: {risk_level}")
                            print(f"   Reason: {reason}")
                        else:  # escalated
                            print(f"⚠️  ESCALATED")
                            print(f"   Risk Level: {risk_level}")
                            print(f"   Reason: {reason}")
                            print(f"   → Waiting for human review")
                        
                        return verdict
                    else:
                        elapsed = int(time.time() - start_time)
                        print(f"   [{elapsed}s] Still processing...")
                        
                time.sleep(poll_interval)
                
            except Exception as e:
                print(f"❌ Poll error: {e}")
                return None
        
        print(f"❌ Verdict not ready after {max_wait}s")
        return None

    def forward_action(self, action_id: str) -> Optional[Dict]:
        """Forward action to target system (only if approved)"""
        print(f"\n🚀 Forwarding action to target system...")
        
        try:
            response = requests.post(
                f"{self.config.deko_url}/action/{action_id}/forward",
                headers=self.headers,
                timeout=30
            )
            
            if response.status_code == 200:
                result = response.json()
                print(f"✅ Action executed successfully!")
                print(f"   Response: {json.dumps(result, indent=2)}")
                return result
            else:
                print(f"❌ Forward failed: {response.status_code}")
                print(f"   Response: {response.text}")
                return None
                
        except Exception as e:
            print(f"❌ Forward error: {e}")
            return None

    def execute_workflow(self, intent: str, payload: Dict[str, Any], 
                        target_url: str) -> bool:
        """Execute full workflow: submit -> poll -> forward"""
        
        # Step 1: Submit action
        action_id = self.submit_action(intent, payload, target_url)
        if not action_id:
            return False
        
        # Step 2: Poll for verdict
        verdict = self.poll_verdict(action_id)
        if not verdict:
            return False
        
        # Step 3: Forward if approved
        if verdict.get("decision") == "approved":
            result = self.forward_action(action_id)
            return result is not None
        else:
            print(f"\n⛔ Action not approved, skipping forward")
            return False


def main():
    """Example usage"""
    
    # Configuration
    config = DekoConfig(
        deko_url="http://localhost:8000",
        admin_password="changeme"
    )
    
    # Create agent
    agent = DekoAgent("test-agent-1", config)
    
    # Register if no API key
    if not agent.api_key:
        if not agent.register():
            print("Failed to register, exiting")
            sys.exit(1)
    
    print("\n" + "="*60)
    print("SCENARIO 1: Approve Legitimate Action")
    print("="*60)
    
    # Scenario 1: Simple legitimate action (should approve)
    agent.execute_workflow(
        intent="Update customer email in CRM",
        payload={
            "customer_id": "cust_001",
            "old_email": "old@example.com",
            "new_email": "new@example.com"
        },
        target_url="https://httpbin.org/post"
    )
    
    print("\n" + "="*60)
    print("SCENARIO 2: Suspicious Action (may escalate/deny)")
    print("="*60)
    
    # Scenario 2: Potentially suspicious action
    time.sleep(2)  # Small delay
    agent.execute_workflow(
        intent="Export all customer database records to external storage",
        payload={
            "table": "customers",
            "format": "csv",
            "destination": "s3://external-bucket/"
        },
        target_url="https://httpbin.org/post"
    )
    
    print("\n" + "="*60)
    print("Done!")
    print("="*60)


if __name__ == "__main__":
    main()
